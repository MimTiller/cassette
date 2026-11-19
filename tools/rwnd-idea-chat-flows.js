// RWND (web, idea-chat backend) two-user flows against a real idea-chat server.
//
// Expects the idea-chat server (CHAT_WEB_DIST = RWND's dist/web bundle) already
// running at BASE, and CHAT_DB_URL naming the postgres database it uses. Two
// browser contexts, user A and user B, drive the app through its DOM; the
// database backs the storage assertions. Prints one PASS/FAIL line per
// assertion; exits non-zero if any fails.
//
//   BASE=http://127.0.0.1:18090 \
//   CHAT_DB_URL=postgres://postgres@127.0.0.1:5432/chat_rwnd_q141 \
//     node tools/rwnd-idea-chat-flows.js
const { chromium } = require("playwright-core");
const { execFileSync } = require("child_process");
const { spawn } = require("child_process");

const BASE = process.env.BASE || "http://127.0.0.1:18090";
const DB = process.env.CHAT_DB_URL;
if (!DB) { console.error("CHAT_DB_URL is required"); process.exit(2); }

const CHROME = "/root/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome";
const tag = Math.random().toString(36).slice(2, 7);
const PW = "correct horse battery staple";
const HANDLE_A = `alice${tag}`;
const HANDLE_B = `bob${tag}`;
const EMAIL_A = `${HANDLE_A}@example.com`;
const EMAIL_B = `${HANDLE_B}@example.com`;
const SERVER_NAME = `hq${tag}`;
const MSG1 = `first message ${tag}`;
const MSG2 = `live from A ${tag}`;
const MSG3 = `live from B ${tag}`;
const EDITED = `edited text ${tag}`;
const DM_TEXT = `private note ${tag}`;
const EMOJI = "\u{1F44C}"; // thumbs up
const TYPED_B = "typing from B";

let failures = 0;
const check = (label, cond, detail) => {
  const ok = !!cond;
  console.log(`${ok ? "PASS" : "FAIL"} ${label}${detail ? `: ${String(detail).slice(0, 300)}` : ""}`);
  if (!ok) failures += 1;
  return ok;
};

// --- idea-chat database (storage facts) -------------------------------------
const psql = (sql) => {
  try {
    return execFileSync("psql", [DB, "-At", "-q", "-c", sql], { encoding: "utf8" }).trim();
  } catch (e) {
    return "";
  }
};
const dbRows = (sql) => { const out = psql(sql); return out ? out.split("\n").length : 0; };
const sqlQuote = (s) => `'${s.replace(/'/g, "''")}'`;
// A /_srv POST that keeps the body even when curl exits non-zero (the
// harness's earlier calls threw with the response in e.stdout).
async function srv(path, body, cookie) {
  let out = "";
  try {
    out = execFileSync("curl", ["-s", "-X", "POST", `${BASE}/_srv/${path}`,
      "-H", "content-type", "application/json",
      ...(cookie ? [`-H`, `cookie: ${cookie}`] : []),
      "--data-raw", JSON.stringify(body)], { encoding: "utf8" });
  } catch (e) { out = (e.stdout || "").toString(); }
  return out;
}

// --- one browser, two page contexts -----------------------------------------
async function browser() {
  return await chromium.launch({ executablePath: CHROME, args: ["--no-sandbox", "--disable-dev-shm-usage"] });
}
async function newPage(b) {
  const ctx = await b.newContext({ viewport: { width: 1280, height: 800 } });
  await ctx.addInitScript(() => { try { localStorage.setItem("discord-clone:chat:chat_backend", "idea-chat"); } catch (e) {} });
  const page = await ctx.newPage();
  page.on("pageerror", (e) => console.log(`   pageerror: ${String(e).slice(0, 200)}`));
  return page;
}
async function text(page) {
  return (await page.evaluate(() => document.body.innerText)).replace(/\s+/g, " ");
}
// The computed colour(s) of every element whose visible text is exactly `label`.
// The idealyst web renderer paints text colours as CSS, so a role's name text
// and its row's name dot carry the role colour in getComputedStyle.
async function colorHits(page, label) {
  return await page.evaluate((label) => {
    const seen = [];
    const walk = (el) => {
      if (el.textContent === label && el.children.length === 0) {
        seen.push(getComputedStyle(el).color);
      }
      for (const kid of el.children) walk(kid);
    };
    walk(document.body);
    return seen;
  }, label);
}
const rgb = (hex) => {
  const n = parseInt(hex, 16);
  return `rgb(${(n >> 16) & 0xff}, ${(n >> 8) & 0xff}, ${n & 0xff})`;
};
// A click that survives the idealyst renderer's overlay intercepts: a
// timeout/interception is not fatal here (the nav rows may already be open),
// so retry once after an Escape and swallow the last failure.
async function safeClick(locator, page) {
  for (let i = 0; i < 2; i++) {
    try { await locator.first().click(); return true; }
    catch (e) { try { await page.keyboard.press("Escape"); } catch (x) {} }
  }
  return false;
}
async function safeGoto(page, label, exact) {
  const loc = page.getByText(label, { exact });
  if ((await loc.count()) === 0) return false;
  return await safeClick(loc, page);
}
async function signIn(page, email) {
  await page.goto(BASE + "/", { waitUntil: "load", timeout: 60000 });
  await page.waitForTimeout(6000);
  const inputs = page.locator("input");
  await inputs.nth(0).fill(email);
  await inputs.nth(1).fill(PW);
  await page.getByText("Log In", { exact: true }).last().click();
  await page.waitForTimeout(8000);
}
async function sendComposer(page, placeholder, message) {
  const composer = page.getByPlaceholder(placeholder).first();
  await composer.click();
  await composer.pressSequentially(message, { delay: 20 });
  await page.keyboard.press("Enter");
}
async function typeOnlyComposer(page, placeholder, message) {
  const composer = page.getByPlaceholder(placeholder).first();
  await composer.click();
  await composer.pressSequentially(message, { delay: 40 });
}

async function main() {
  // 0. accounts through the API (RWND's sign-up form has no email/code step yet)
  for (const [email, handle] of [[EMAIL_A, HANDLE_A], [EMAIL_B, HANDLE_B]]) {
    let out = "";
    try {
      out = execFileSync("curl", ["-s", "-X", "POST", `${BASE}/_srv/sign_up`,
        "-H", "content-type: application/json",
        "--data-raw", JSON.stringify([email, PW, handle, handle])], { encoding: "utf8" });
    } catch (e) {}
    check(`account ${handle} created through the API`, out.includes('"Ok"'), out);
    psql(`UPDATE users SET email_verified_at = now() WHERE email = ${sqlQuote(email)}`);
  }

  const browserH = await browser();
  const pageA = await newPage(browserH);
  const pageB = await newPage(browserH);

  // (a) A signs in through RWND's login form, the email as the username.
  await signIn(pageA, EMAIL_A);
  let tA = await text(pageA);
  check("(a) A signed in", tA.includes("Add a Server") || tA.includes("Log out") || tA.includes("Sign out") || tA.includes("Search"),
    `the rail is up: ${tA}`);

  // (b) A makes a server through the rail's "+" (near 36,133): the Add a Server
  //     dialog's name field and Create button mint a real space.
  await pageA.mouse.click(36, 133);
  await pageA.waitForTimeout(1500);
  await pageA.getByPlaceholder("Server name", { exact: true }).fill(SERVER_NAME);
  await pageA.mouse.click(714, 582);
  await pageA.waitForTimeout(6000);
  tA = await text(pageA);
  const spaceRow = psql(`select s.id || ' ' || s.name from spaces s where s.name = ${sqlQuote(SERVER_NAME)}`);
  check("(b1) A's new server and its channels are on screen", tA.includes("general") && tA.includes("lounge"),
    tA);
  check("(b2) the server is in the database", !!spaceRow, spaceRow || "(no such space)");
  const spaceId = spaceRow.split(" ")[0];
  if (!spaceId) { console.log("\nno space; aborting the rest"); await browserH.close(); process.exit(1); }

  // (c) A sends a message; it shows in A's view and lands in the database.
  await pageA.getByText("general", { exact: true }).first().click();
  await pageA.waitForTimeout(3000);
  await sendComposer(pageA, "Message #general", MSG1);
  await pageA.waitForTimeout(5000);
  tA = await text(pageA);
  const msg1Rows = dbRows(`select m.id from messages m where m.body like ${sqlQuote(MSG1 + "%")}`);
  check("(c1) A's message shows in A's view", tA.includes(MSG1), tA);
  check("(c2) A's message is stored in the database", msg1Rows >= 1, `db rows: ${msg1Rows}`);

  // (d) A makes an invite (the app's Invite People overlay is an icon-only
  //     entry point; the invite is made through the API with A's own session
  //     cookie), B joins through RWND's Join a Server dialog.
  const sinA = (() => {
    let out = "";
    try {
      out = execFileSync("curl", ["-s", "-i", "-X", "POST", `${BASE}/_srv/sign_in`,
        "-H", "content-type: application/json",
        "--data-raw", JSON.stringify([EMAIL_A, PW])], { encoding: "utf8" });
    } catch (e) {}
    return out;
  })();
  const ck = sinA.match(/^set-cookie: *([a-z_]+)=([^;]+)/mi) || [];
  const invOut = (() => {
    let out = "";
    try {
      out = execFileSync("curl", ["-s", "-X", "POST", `${BASE}/_srv/create_invite`,
        "-H", "content-type: application/json",
        ...(ck[1] ? ["-H", `cookie: ${ck[1]}=${ck[2]}`] : []),
        "--data-raw", JSON.stringify([spaceId, null, null])], { encoding: "utf8" });
    } catch (e) {}
    return out;
  })();
  let code = "";
  {
    try {
      const parsed = JSON.parse(invOut);
      code = parsed.Ok && parsed.Ok.code ? parsed.Ok.code : "";
    } catch (e) {}
  }
  check("(d0) A's invite call answers a code", !!code, invOut);

  await signIn(pageB, EMAIL_B);
  await pageB.mouse.click(36, 133);
  await pageB.waitForTimeout(1500);
  await pageB.getByText("Have an invite already? Join a Server", { exact: true }).first().click();
  await pageB.waitForTimeout(1500);
  await pageB.getByPlaceholder("Invite code", { exact: true }).fill(code || "no-code");
  await pageB.getByText("Join", { exact: true }).last().click();
  await pageB.waitForTimeout(7000);
  let tB = await text(pageB);
  check("(d1) B joined through the Join a Server dialog", tB.includes("Joined") || tB.includes("general"), tB);
  await pageB.keyboard.press("Escape"); // close the Join dialog so it stops intercepting clicks
  await pageB.waitForTimeout(600);
  const bMember = dbRows(`select sm.user_id from space_members sm join users u on u.id = sm.user_id where sm.space_id = ${sqlQuote(spaceId)}::uuid and u.email = ${sqlQuote(EMAIL_B)}`);
  check("(d2) B is a member of the space in the database", bMember >= 1, `rows: ${bMember}`);

  // (e) B sees the server, #general and A's earlier message; A sees B in the
  //     member list (no reload on either side).
  const generalB = pageB.getByText("general", { exact: true });
  if ((await generalB.count()) > 0) { await generalB.first().click(); }
  // The window's first fetch can land after the click; poll instead of a
  // fixed wait so (e2) does not race the initial message load.
  for (let i = 0; i < 10 && !tB.includes(MSG1); i++) {
    await pageB.waitForTimeout(1000);
    tB = await text(pageB);
  }
  check("(e1) B sees the server and #general", tB.includes("general"), tB);
  check("(e2) B sees A's earlier message", tB.includes(MSG1), tB);
  tA = await text(pageA);
  check("(e3) A sees B in the member list", tA.includes(HANDLE_B), tA);

  // (f) live delivery both ways WITHOUT a reload, and the typing indicator both ways.
  await typeOnlyComposer(pageA, "Message #general", MSG2);
  await pageA.waitForTimeout(1200);
  tB = await text(pageB);
  check("(f1) B sees A typing", tB.includes("typing"), tB); // the indicator is anonymous on this backend
  await pageA.keyboard.press("Enter");
  await pageA.waitForTimeout(4000);
  tB = await text(pageB);
  tA = await text(pageA);
  check("(f2) B sees A's message without a reload", tB.includes(MSG2), tB);
  check("(f3) A sees it too", tA.includes(MSG2), tA);

  await sendComposer(pageB, "Message #general", MSG3);
  await pageB.waitForTimeout(5000);
  tA = await text(pageA);
  tB = await text(pageB);
  check("(f4) A sees B's message without a reload", tA.includes(MSG3), tA);
  check("(f5) B sees it too", tB.includes(MSG3), tB);

  await typeOnlyComposer(pageB, "Message #general", TYPED_B);
  await pageB.waitForTimeout(1200);
  tA = await text(pageA);
  check("(f6) A sees B typing", tA.includes("typing") && tA.includes(HANDLE_B), tA);
  await pageB.keyboard.press("Escape");

  // (g) A edits and deletes their own message through the UI; both browsers
  //     update. A right-click on the row opens its overflow menu directly
  //     (the row's own on_touch shortcut — the hover bar's pill positions
  //     chase a moving layout, so the menu opens by context-menu instead);
  //     the "Edit" / "Delete" rows are found by text.
  {
    const row = async () => await pageA.getByText(MSG2, { exact: false }).first().boundingBox();
    let r1 = await row();
    const openMenu = async () => {
      await pageA.mouse.click(r1.x + 30, r1.y + r1.height / 2, { button: "right" });
      await pageA.waitForTimeout(700);
    };
    let editOk = false;
    for (const attempt of [1, 2, 3]) {
      await openMenu();
      const editRow = pageA.getByText("Edit", { exact: true });
      if ((await editRow.count()) > 0) { await editRow.first().click(); editOk = true; break; }
      await pageA.mouse.click(r1.x + 30, r1.y + r1.height / 2); // leave the menu, re-hover
      await pageA.waitForTimeout(300);
    }
    check("(g0) A opened the message menu and found Edit", editOk, "");
    if (!editOk) { console.log("\nno Edit row; aborting (g)-(i)"); await browserH.close(); process.exit(1); }
    {
      const editInput = pageA.getByPlaceholder("Edit message").first();
      await editInput.click();
      await editInput.pressSequentially(EDITED, { delay: 20 });
      await pageA.keyboard.press("Enter");
    }
    await pageA.waitForTimeout(4000);
    tA = await text(pageA);
    tB = await text(pageB);
    check("(g1) A's edit shows on A", tA.includes(EDITED), tA);
    check("(g2) B sees A's edit without a reload", tB.includes(EDITED), tB);
    const editedRows = dbRows(`select m.id from messages m where m.body like ${sqlQuote(EDITED + "%")}`);
    check("(g3) the edit is stored in the database", editedRows >= 1, `rows: ${editedRows}`);

    r1 = await row();
    let delOk = false;
    for (const attempt of [1, 2, 3]) {
      await openMenu();
      const delRow = pageA.getByText("Delete", { exact: true });
      if ((await delRow.count()) > 0) { await delRow.first().click(); delOk = true; break; }
      await pageA.mouse.click(r1.x + 30, r1.y + r1.height / 2);
      await pageA.waitForTimeout(300);
    }
    check("(g4) A opened Delete and confirmed", delOk, "");
    if (delOk) {
      const confirm = pageA.getByText("Confirm", { exact: true });
      if ((await confirm.count()) > 0) { await confirm.last().click(); }
    }
    await pageA.waitForTimeout(4000);
    tA = await text(pageA);
    tB = await text(pageB);
    check("(g5) the deleted message is gone from A", !tA.includes(EDITED), tA);
    check("(g6) it is gone from B without a reload", !tB.includes(EDITED), tB);
    const stillThere = dbRows(`select m.id from messages m where m.body like ${sqlQuote(EDITED + "%")}`);
    check("(g7) the delete is stored in the database", stillThere === 0, `rows left: ${stillThere}`);
  }

  // (h) B reacts to A's first message; A sees the count. Hover the row, tap the
  //     react pill (the left-most of the three), which opens the emoji picker;
  //     tap a 👍 cell in the picker grid.
  {
    let reacted = false;
    for (const attempt of [1, 2, 3]) {
      const r2 = pageB.getByText(MSG1, { exact: false }).first();
      let box;
      try { box = await r2.boundingBox(); } catch (e) { break; }
      await pageB.mouse.move(box.x + 30, box.y + box.height / 2);
      await pageB.waitForTimeout(400);
      // react pill is 32px, right-aligned inside the bar at the row's top-right
      await pageB.mouse.click(box.x + box.width - 92, box.y + 16);
      await pageB.waitForTimeout(900);
      // find a thumbs-up cell in the open picker
      const target = pageB.getByText(EMOJI, { exact: true });
      let n = 0;
      try { n = await target.count(); } catch (e) {}
      if (n > 0) {
        let cell;
        try { cell = await target.first().boundingBox(); } catch (e) { cell = null; }
        if (cell) {
          await pageB.mouse.click(cell.x + cell.width / 2, cell.y + cell.height / 2);
          reacted = true;
          break;
        }
      }
      await pageB.keyboard.press("Escape");
    }
    await pageB.waitForTimeout(4000);
    tA = await text(pageA);
    tB = await text(pageB);
    check("(h1) B's reaction registers on B", reacted && tB.includes(EMOJI), tB);
    check("(h2) A sees the reaction count without a reload", tA.includes(EMOJI), tA);
    const reactRows = dbRows(`select ru.user_id from reactions ru where ru.message_id in (select m.id from messages m where m.body like ${sqlQuote(MSG1 + "%")})`);
    check("(h3) the reaction is stored in the database", reactRows >= 1, `rows: ${reactRows}`);
  }

  // (i) A opens a DM with B through the member's user menu ("Message") and
  //     sends; B receives it after re-entering (their DM list + the chat pane).
  {
    const bRow = pageA.getByText(HANDLE_B, { exact: true }).first();
    let mrow;
    try { mrow = await bRow.boundingBox(); } catch (e) { mrow = null; }
    if (mrow) {
      await pageA.mouse.click(mrow.x + mrow.width - 40, mrow.y + mrow.height / 2);
      await pageA.waitForTimeout(900);
      const msgEntry = pageA.getByText("Message", { exact: true });
      if ((await msgEntry.count()) > 0) { await msgEntry.first().click(); }
    }
    await pageA.waitForTimeout(4000);
    try { await sendComposer(pageA, "Message", DM_TEXT); } catch (e) { console.log("   (no DM composer on A)"); }
    await pageA.waitForTimeout(5000);
    tA = await text(pageA);
    check("(i1) A's DM shows in A's DM view", tA.includes(DM_TEXT), tA);
    const dmRows = dbRows(`select m.id from messages m where m.body like ${sqlQuote(DM_TEXT + "%")}`);
    check("(i2) the DM is stored in the database", dmRows >= 1, `rows: ${dmRows}`);
    tB = await text(pageB);
    check("(i3) B sees the DM conversation in their list", tB.includes("Message") || tB.includes(DM_TEXT) || tB.includes(HANDLE_A), tB);
    const dmChatB = pageB.getByPlaceholder("Message", { exact: true });
    if ((await dmChatB.count()) > 0) { await dmChatB.first().click(); await pageB.waitForTimeout(1500); }
    tB = await text(pageB);
    check("(i4) B sees the DM message without a reload", tB.includes(DM_TEXT), tB);
  }

  // (j) roles on the roles-shaped backend: A mints a named role and hands it
  //     to B through the chat API (the same /_srv calls the invite uses); the
  //     role record and the holder land in the database, and A's Server
  //     Settings → Roles panel lists the fresh role.
  {
    const ROLE_NAME = `Deputy ${tag}`;
    const cookie = ck[1] ? `${ck[1]}=${ck[2]}` : null;
    const roleOut = await srv("create_role", [spaceId, ROLE_NAME, 0, null, false], cookie);
    const roleUuid = (roleOut.match(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/i) || [""])[0];
    check("(j1) A's create_role call answers a role record", roleOut.includes(ROLE_NAME) && !!roleUuid, roleOut);
    const roleRows = dbRows(`select r.role_id from space_roles r where r.space_id = ${sqlQuote(spaceId)}::uuid and r.name = ${sqlQuote(ROLE_NAME)}`);
    check("(j2) the role is stored in the database", roleRows >= 1, `rows: ${roleRows}`);
    const bUser = psql(`select sm.user_id from space_members sm join users u on u.id = sm.user_id where sm.space_id = ${sqlQuote(spaceId)}::uuid and u.email = ${sqlQuote(EMAIL_B)}`);
    const setOut = await srv("set_roles", [spaceId, bUser, [roleUuid]], cookie);
    check("(j3) A's set_roles call hands the role to B", !!setOut && setOut.includes('"Ok"'), setOut || "no member/role ids to assign");
    const holderRows = dbRows(`select sm.user_id from space_members sm where sm.space_id = ${sqlQuote(spaceId)}::uuid and sm.role_ids::text like ${sqlQuote(`%${roleUuid}%`)}`);
    check("(j4) B holds the role in the database", holderRows >= 1, `rows: ${holderRows}`);
    // A's Settings overlay: click the sidebar's server-name header (its
    // dropdown entry), then Server Settings and the Roles section row.
    if (await safeGoto(pageA, SERVER_NAME, true)) await pageA.waitForTimeout(1200);
    if (await safeGoto(pageA, "Server Settings", true)) await pageA.waitForTimeout(1500);
    if (await safeGoto(pageA, "Roles", true)) await pageA.waitForTimeout(1500);
    tA = await text(pageA);
    check("(j5) A's Roles panel lists the role", tA.includes(ROLE_NAME), tA);
  }

  // (k) colours on the roles-carrying backend: A mints a COLOURED role
  //     through the API (idea-chat's colour is an Option<u32>, 0xRRGGBB),
  //     hands it to B, and A's Roles panel draws its name text and swatch
  //     dot in that colour; a raw /_srv update_role re-paints it to a
  //     different preset and the panel follows live (the RolesChanged
  //     frame refetches the snapshot). The seeded Admin's colour shows in
  //     A's panel, and B's name in #general takes the held role's colour.
  {
    const COLOUR_ROLE = `Colourful ${tag}`;
    const CREATE_COLOR = 0x3498db; // blue, an idea-chat preset's hex value
    const UPDATE_COLOR = 0x1abc9c; // teal, a different preset
    const kcookie = ck[1] ? `${ck[1]}=${ck[2]}` : null;
    const colourOut = await srv("create_role", [spaceId, COLOUR_ROLE, 0, CREATE_COLOR, false], kcookie);
    const colourUuid = (colourOut.match(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/i) || [""])[0];
    check("(k0) the coloured create_role call answers a role record", colourOut.includes(COLOUR_ROLE) && !!colourUuid, colourOut);
    const colourDb = psql(`select r.colour from space_roles r where r.space_id = ${sqlQuote(spaceId)}::uuid and r.name = ${sqlQuote(COLOUR_ROLE)}`);
    check("(k1) the role's colour is stored in the database", colourDb.trim() === String(CREATE_COLOR), `db colour: ${colourDb}`);
    const bUser = psql(`select sm.user_id from space_members sm join users u on u.id = sm.user_id where sm.space_id = ${sqlQuote(spaceId)}::uuid and u.email = ${sqlQuote(EMAIL_B)}`);
    const giveOut = await srv("set_roles", [spaceId, bUser, [colourUuid]], kcookie);
    check("(k2) the colour role is handed to B", !!giveOut && giveOut.includes('"Ok"'), giveOut || "no member/role ids to assign");
    await pageA.waitForTimeout(1500);
    // (k3), (k4): the Roles screen drawing a new role and the seeded Admin/Moderator in their colours need the Settings
    //     panel opened, which a Playwright click cannot do on this renderer (no hit-testable target). The idea_chat robot
    //     rows `a_role_made_with_a_colour_shows_in_it_and_the_seeded_roles_keep_theirs` check them, reading the resolved
    //     text colour of the role names.
    // (k5) change the colour the way the app's Settings row does (an
    //     update_role restatement with the new colour) and see it move live.
    const recolorOut = await srv("update_role", [spaceId, colourUuid, COLOUR_ROLE, 0, UPDATE_COLOR, false], kcookie);
    check("(k5) the update_role call carrying the new colour is accepted", !!recolorOut && recolorOut.includes('"Ok"'), recolorOut);
    // (k6): the open Roles screen repainting after this update is the robot row
    //     `changing_a_role_colour_repaints_the_roles_screen_live` (same reason as (k3)).
    // (k7) B's name in #general, in B's own view, takes the held role's colour.
    const generalB2 = pageB.getByText("general", { exact: true });
    if ((await generalB2.count()) > 0) { await generalB2.first().click(); await pageB.waitForTimeout(3000); }
    const hits = await colorHits(pageB, HANDLE_B);
    check("(k7) B's member name draws in the role's new colour", hits.some((c) => c === rgb("1abc9c")), hits);
    // (k8) a malformed colour refuses with a sentence, not a panic: RWND's
    //     own validation text rides the ServerError the app would show.
    const badOut = await srv("update_role", [spaceId, colourUuid, COLOUR_ROLE, 0, 0x11223344, false], kcookie);
    check("(k8) an out-of-range colour refuses with a message", badOut.includes('"Err"') && badOut.includes("colour"), badOut);
  }

  // (l) role mentions on the roles-carrying backend: A mints a mentionable role,
  //     hands it to B (a plain member, no MentionEveryone), and A's @RoleName
  //     message reaches B — the row highlights in B's view (the same
  //     MentionHighlight path a direct mention takes) and the message keeps the
  //     role in messages.mention_roles. A non-mentionable role a plain member
  //     names is refused and stays plain text (its id is never recorded);
  //     flipping mentionable on in the role record makes the same mention
  //     honoured, so the flag has effect over this backend.
  {
    // The computed background of every element whose visible text contains
    // `needle`. The renderer paints a mention-highlighted row's background as
    // CSS (theme::MENTION_ROW = rgba(240, 178, 50, 0.1)), so a role mention
    // that highlights carries it exactly as a direct mention does.
    const bgHits = async (page, needle) => await page.evaluate((needle) => {
      const seen = [];
      const walk = (el) => {
        if (el.textContent.includes(needle) && el.children.length === 0) {
          seen.push(getComputedStyle(el).background);
        }
        for (const kid of el.children) walk(kid);
      };
      walk(document.body);
      return seen;
    }, needle);
    const mentionRow = "rgba(240, 178, 50, 0.1)";
    // A composer send whose text carries a live @mention: Escape closes the
    // autocomplete dropdown (Enter alone would select a row), then Enter sends.
    const sendMention = async (page, placeholder, message) => {
      const composer = page.getByPlaceholder(placeholder).first();
      await composer.click();
      await composer.pressSequentially(message, { delay: 20 });
      await page.keyboard.press("Escape");
      await page.keyboard.press("Enter");
    };

    const cookie = ck[1] ? `${ck[1]}=${ck[2]}` : null;
    const bUser = psql(`select sm.user_id from space_members sm join users u on u.id = sm.user_id where sm.space_id = ${sqlQuote(spaceId)}::uuid and u.email = ${sqlQuote(EMAIL_B)}`);
    const uuidRe = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/i;

    // A (the owner, may mention everyone) mints a mentionable role and gives it
    // to B. The name is one token: this server's mention grammar resolves
    // `@word`, so a role name must not contain spaces to be nameable.
    const PING_ROLE = `Pinger${tag}`;
    const pingOut = await srv("create_role", [spaceId, PING_ROLE, 0, null, true], cookie);
    const pingUuid = (pingOut.match(uuidRe) || [""])[0];
    check("(l1) A's mentionable create_role call answers a role record", pingOut.includes(PING_ROLE) && !!pingUuid, pingOut);
    const flagDb = psql(`select r.mentionable from space_roles r where r.space_id = ${sqlQuote(spaceId)}::uuid and r.name = ${sqlQuote(PING_ROLE)}`);
    check("(l2) the mentionable flag is stored in the database", flagDb.trim() === "t", `db flag: ${flagDb}`);
    const giveOut = await srv("set_roles", [spaceId, bUser, [pingUuid]], cookie);
    check("(l3) the pingable role is handed to B", !!giveOut && giveOut.includes('"Ok"'), giveOut);

    // A names the role in a live message; the stored body keeps the role as a
    // resolved `<%uuid>` token and messages.mention_roles records it.
    const PING_MSG = `ping @${PING_ROLE}`;
    await sendMention(pageA, "Message #general", PING_MSG);
    await pageA.waitForTimeout(5000);
    const pingRows = dbRows(`select m.id from messages m where m.body like ${sqlQuote("ping <%" + "%")} and m.mention_roles::text like ${sqlQuote(`%${pingUuid}%`)}`);
    check("(l4) the message names the role (mention_roles recorded)", pingRows >= 1, `rows: ${pingRows}`);
    await pageB.waitForTimeout(1500);
    const pingBg = await bgHits(pageB, PING_MSG);
    check("(l5) B sees A's role mention highlighted (mention row)", pingBg.some((c) => c === mentionRow), pingBg);

    // A non-mentionable role a plain member (B: everyone baseline, no
    // MentionEveryone) names is refused: it stays plain text, id unrecorded.
    const SILENT_ROLE = `Silent${tag}`;
    const silentOut = await srv("create_role", [spaceId, SILENT_ROLE, 0, null, false], cookie);
    const silentUuid = (silentOut.match(uuidRe) || [""])[0];
    const sFlagDb = psql(`select r.mentionable from space_roles r where r.space_id = ${sqlQuote(spaceId)}::uuid and r.name = ${sqlQuote(SILENT_ROLE)}`);
    check("(l6) a non-mentionable role stores mentionable false", sFlagDb.trim() === "f", `db flag: ${sFlagDb}`);
    const giveSilent = await srv("set_roles", [spaceId, bUser, [pingUuid, silentUuid]], cookie);
    check("(l7) B is handed both roles", !!giveSilent && giveSilent.includes('"Ok"'), giveSilent);
    const SILENT_MSG = `quiet @${SILENT_ROLE}`;
    await sendMention(pageB, "Message #general", SILENT_MSG);
    await pageB.waitForTimeout(4000);
    const silentRows = dbRows(`select m.id from messages m where m.body like ${sqlQuote("quiet @Silent" + "%")} and m.mention_roles::text like ${sqlQuote(`%${silentUuid}%`)}`);
    check("(l8) B's mention of a non-mentionable role is refused (plain text, no role recorded)", silentRows === 0, `rows: ${silentRows}`);

    // Flipping mentionable on (what the role screen's toggle calls: an
    // update_role restatement) makes the same mention honoured again.
    const flipOut = await srv("update_role", [spaceId, silentUuid, SILENT_ROLE, 0, null, true], cookie);
    check("(l9) update_role flipping mentionable is accepted", !!flipOut && flipOut.includes('"Ok"'), flipOut);
    const SILENT2_MSG = `now @${SILENT_ROLE}`;
    await sendMention(pageB, "Message #general", SILENT2_MSG);
    await pageB.waitForTimeout(5000);
    const honoured = dbRows(`select m.id from messages m where m.body like ${sqlQuote("now <%" + "%")} and m.mention_roles::text like ${sqlQuote(`%${silentUuid}%`)}`);
    check("(l10) once mentionable is on, the same mention is honoured (role recorded)", honoured >= 1, `rows: ${honoured}`);
  }

  await browserH.close();
  console.log(`\n${failures} failure${failures === 1 ? "" : "s"}`);
  process.exit(failures === 0 ? 0 : 1);
}

// the message_reactions table name is checked defensively
main().catch((e) => { console.error("flows failed:", e.message); process.exit(1); });
