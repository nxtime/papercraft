//! The browser's `fetch` as the transport of a [`super::own`] turn: a streamed POST straight to
//! the provider, with the person's own key. Stopping aborts the request.

use super::own::{Calls, Halt, Prepared, Transport, take_utf8};
use crate::mcp::Out;
use serde_json::Value;
use std::cell::RefCell;
use std::sync::{Arc, Mutex, PoisonError};
use std::sync::atomic::{AtomicBool, Ordering};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{AbortController, Headers, ReadableStreamDefaultReader, RequestInit, Response};

thread_local! {
    /// The request of the running turn; the browser is single threaded, so a stop from the UI can
    /// reach it without the controller crossing threads.
    static ABORT: RefCell<Option<AbortController>> = const { RefCell::new(None) };
}

/// Stops the running request; the turn then ends as stopped.
pub fn abort() {
    ABORT.with(|a| {
        if let Some(c) = a.borrow().as_ref() {
            c.abort();
        }
    });
}

pub struct Fetch {
    pub stop: Arc<AtomicBool>,
    pub calls: Arc<Mutex<Calls>>,
    /// Wakes the UI so it sees new events and tool calls.
    pub wake: Box<dyn Fn()>,
    pub label: String,
    pub history: std::path::PathBuf,
}

fn js_text(e: &JsValue) -> String {
    e.as_string().or_else(|| js_sys::Reflect::get(e, &"message".into()).ok().and_then(|m| m.as_string())).unwrap_or_else(|| "unknown error".into())
}

/// The message of a provider's JSON error body, or the body itself when it is not JSON.
fn error_message(body: &str) -> String {
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    v["error"]["message"].as_str().or_else(|| v["error"].as_str()).or_else(|| v["message"].as_str()).map_or_else(|| body.chars().take(300).collect(), str::to_string)
}

impl Fetch {
    fn gone(&self, e: &JsValue) -> Halt {
        if self.stop.load(Ordering::SeqCst) {
            return Halt::Stopped;
        }
        // A blocked cross-origin request gives no detail, so name the likely causes.
        Halt::Fail(format!("{} could not be reached ({}). Check the address and the key, and that the provider accepts requests from a web page.", self.label, js_text(e)))
    }

    async fn send(&self, p: &Prepared) -> Result<Response, Halt> {
        let headers = Headers::new().map_err(|e| self.gone(&e))?;
        headers.set("content-type", "application/json").map_err(|e| self.gone(&e))?;
        for (k, v) in &p.headers {
            headers.set(k, v).map_err(|e| Halt::Fail(format!("invalid {k} header: {}", js_text(&e))))?;
        }
        let ctl = AbortController::new().map_err(|e| self.gone(&e))?;
        let init = RequestInit::new();
        init.set_method("POST");
        init.set_headers(&headers);
        init.set_body(&JsValue::from_str(&p.body.to_string()));
        init.set_signal(Some(&ctl.signal()));
        ABORT.with(|a| *a.borrow_mut() = Some(ctl));
        let window = web_sys::window().ok_or_else(|| Halt::Fail("no browser window".into()))?;
        let resp = JsFuture::from(window.fetch_with_str_and_init(&p.url, &init)).await.map_err(|e| self.gone(&e))?;
        let resp: Response = resp.dyn_into().map_err(|e| self.gone(&e))?;
        if resp.ok() {
            return Ok(resp);
        }
        let status = resp.status();
        let body = match resp.text() {
            Ok(t) => JsFuture::from(t).await.ok().and_then(|v| v.as_string()).unwrap_or_default(),
            Err(_) => String::new(),
        };
        Err(Halt::Fail(format!("{} error {status}: {}", self.label, error_message(&body))))
    }
}

impl Transport for Fetch {
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    async fn post(&self, p: &Prepared, feed: &mut dyn FnMut(&str) -> bool) -> Result<(), Halt> {
        let resp = self.send(p).await?;
        let body = resp.body().ok_or_else(|| Halt::Fail(format!("{} sent no body", self.label)))?;
        let reader: ReadableStreamDefaultReader = body.get_reader().dyn_into().map_err(|e| self.gone(&e))?;
        let mut pending = vec![];
        loop {
            if self.stopped() {
                return Err(Halt::Stopped);
            }
            let chunk = JsFuture::from(reader.read()).await.map_err(|e| self.gone(&e))?;
            if js_sys::Reflect::get(&chunk, &"done".into()).ok().and_then(|d| d.as_bool()).unwrap_or(true) {
                return Ok(());
            }
            let value = js_sys::Reflect::get(&chunk, &"value".into()).map_err(|e| self.gone(&e))?;
            pending.extend(js_sys::Uint8Array::new(&value).to_vec());
            let text = take_utf8(&mut pending);
            if !text.is_empty() && feed(&text) {
                let _ = reader.cancel();
                return Ok(());
            }
        }
    }

    async fn call(&self, name: &str, args: &Value) -> Option<Result<Vec<Out>, String>> {
        let rx = self.calls.lock().unwrap_or_else(PoisonError::into_inner).submit(name, args);
        (self.wake)();
        Some(rx.await.unwrap_or_else(|_| Err("the app dropped the tool call".into())))
    }

    fn persist(&self, messages: &[Value]) {
        super::tools::persist(Some(&self.history), messages);
    }
}
