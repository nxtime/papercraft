# DS report (Design System page 64e1d4aaf56fe6a4)

## Component ids (all top-level components, import with import_components)
| id | name |
|---|---|
| 64e1d4:10899 | Button / SSO / Default |
| 64e1d4:10903 | Button / SSO / Hover |
| 64e1d4:10907 | Button / SSO / Loading |
| 64e1d4:10911 | Button / SSO / Disabled |
| 64e1d4:10915 | Tag / Candidate type / Internal |
| 64e1d4:10919 | Tag / Candidate type / External |
| 64e1d4:10923 | Tag / Candidate type / Internal Department |
| 64e1d4:10927 | Directory picker / Results |
| 64e1d4:10953 | Directory picker / No results |
| 64e1d4:10963 | Directory picker / Loading |
| 64e1d4:10983 | Identity chip |
| 64e1d4:10991 | SSO status / Redirecting |
| 64e1d4:11000 | SSO status / Cancelled |
| 64e1d4:11009 | SSO status / Not allowed |
| 64e1d4:11018 | SSO status / Expired |
| 64e1d4:11027 | SSO status / IdP unavailable |

Dev handoff: real annotations (template Dev handoff, dev/behaviour + a11y, 2 each) on 10899, 10915, 10983, 10927, 10991. Old plain frames 11036/11041/11046/11050/11055 deleted.

## Notes
- Placement: SSO buttons and Identity chip beside Avatars row; tags right of the Tag row; picker states in the forms zone (x 4400, y 3397); SSO status right of Alerts (x 4200, y 5097).
- Built only from existing variables (tag blue/purple, brand, text, border, bg) and instances Avatar / 36, Avatar / 40, Button / Ghost|Primary|Secondary / SM. No new variables.
- Internal = blue + buildings icon, External = purple + user-circle icon (never colour only). Contrast OK (lint 0 errors).
- Hover state: same fill, 2px brand-hover border, underlined label (no darker brand variable exists). Disabled uses text-secondary on bg-layout for 4.5:1.
- Avatar initials in picker/chip are instance overrides (AP, AS, AL).
- Lint: 0 errors; only the "40px tall" warning on SSO buttons (same as Button / * / LG).
- Google/Microsoft: find_nodes on text found none; no logo components or buttons on the DS page. Nothing removed.
- Tidy: frame-scoped tidy on Section / Components (64e1d4:7475) returned nothing (components live outside that frame), so tidy ran page-wide.
- Open: Directory picker shows no selected-state component; Not allowed card may need the allowed-domain hint (? product).
