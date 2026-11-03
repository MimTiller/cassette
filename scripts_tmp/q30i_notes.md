# q30i notes (append per screen)

## login (after)
- Card now 480px on decorated canvas: two role washes (top-left indigo disc, bottom-right near-invisible), two faint cassette motifs. Motifs repositioned to corners (new commit 13566cc). Bottom-right wash nearly invisible on dark = contrast fail.
- Form/box/button now share one inset column (matches item 13 done). USERNAME/PASSWORD labels caps. Input boxes recessed, still NO visible 1px border on dark at this size (borderless_input vs bordered helper — check), placeholder "username" gray reads as value-ish, password mask dots longer (good).
- Tab pair: active indigo underline + inactive gray underline now equal width joined as one rule (fix attempt; underline looks like full-width rule with left segment colored — acceptable).
- Log In button fits field column, full width of that column. Card floats with drop shadow on scrim? Card-vs-canvas contrast still low but shadow present. Error line red sits between forgot link and button when validation fires — fine.
- Login card vertical center: card centered in canvas (was upper-middle).

## main-view-dark (1600x900, empty general@214? no: initial open channel general top-level 214 empty)
- Welcome block pinned top of feed, composer bottom-pinned; middle void ~70% remains for the EMPTY top-level #general. Item 14 (empty states) did the Discord 'Welcome to #name!' block but does not fill/center for sparse channels — still a big void.
- Left server rail CLIPPED at canvas edge (rail icons bleed off-canvas — item 15 NOT fixed; still present): "Cassette/Demo" wraps over server logo, green + button partially cut. STRAY white vertical bar on far-left edge (regression? rail overflow).
- Sidebar vs center boundary: sidebar slightly lighter than canvas; 1px edge present; improved over before but low delta.
- Header trio: pin/people icons now 20px (commit 8a0008c) with search field at right — spacing still uneven; funnel/search order differs from before (funnel icon appears before member icons: filter icon present).
- Member rail: ONLINE — 1 / OFFLINE — 2 caps with counts, presence dots now: you=yellow? (should be green), offline hollow gray rings (commit f9fc0a9). Names still no role colour (Night Watch etc not in this rail? Avery North white).
- Empty sidebar void between last channel and footer — unfixed (settings-void family for sidebar).

## empty-channel (#quiet-corner)
- Same welcome-block treatment; void persists. Composer bottom pinned.
- Left rail glitch: clipped duplicate sidebar sliver with Cass/Demo text bleeding, green + clipped.

## server-dropdown
- Dropdown now elevated card: lighter fill + border + shadow (commit 57ec968) — hole-punch fixed.
- Caret + dropdown overflow sidebar right edge into content pane (dropdown panel extends past sidebar divider). Still two-column collision.
- Rows text-only still (no icons) — item deferred.
- NOTE DB seed: server 168 has TWO 'general' channels (top-level 214 + under Mixtapes 217) — seed data artifact visible as duplicate row, not app bug.

## invite-dialog
- Modal now two-tone: dark header strip (title + Done pill) + lighter body card on scrim — elevation family applied (453f210).
- INVITE EXPIRY radio rows w/ 30 minutes selected; Create button right-aligned; GAP between expiry panel and Create footer panel (card reads as split); Create flush to top of footer band. No invite code/link chip shown before creating — acceptable flow (expires then create).
- 'Done' header pill close affordance present.
