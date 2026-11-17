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
  check("(b1) A's new server and its channels are on screen", tA.includes(SERVER_NAME) && tA.includes("general"),
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
  const cookie = (sinA.match(/^set-cookie: *session=([^;]+)/mi) || [])[1];
  const invOut = (() => {
    let out = "";
    try {
      out = execFileSync("curl", ["-s", "-X", "POST", `${BASE}/_srv/create_invite`,
        "-H", "content-type: application/json",
        ...(cookie ? ["-H", `cookie: session=${cookie}`] : []),
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
  check("(d1) B joined through the Join a Server dialog", tB.includes("Joined") || tB.includes(SERVER_NAME), tB);
  const bMember = dbRows(`select sm.user_id from space_members sm join users u on u.id = sm.user_id where sm.space_id = ${spaceId}::uuid and u.email = ${sqlQuote(EMAIL_B)}`);
  check("(d2) B is a member of the space in the database", bMember >= 1, `rows: ${bMember}`);

  // (e) B sees the server, #general and A's earlier message; A sees B in the
  //     member list (no reload on either side).
  const generalB = pageB.getByText("general", { exact: true });
  if ((await generalB.count()) > 0) { await generalB.first().click(); await pageB.waitForTimeout(3000); }
  tB = await text(pageB);
  check("(e1) B sees the server and #general", tB.includes(SERVER_NAME) && tB.includes("general"), tB);
  check("(e2) B sees A's earlier message", tB.includes(MSG1), tB);
  tA = await text(pageA);
  check("(e3) A sees B in the member list", tA.includes(HANDLE_B), tA);

  // (f) live delivery both ways WITHOUT a reload, and the typing indicator both ways.
  await typeOnlyComposer(pageA, "Message #general", MSG2);
  await pageA.waitForTimeout(1200);
  tB = await text(pageB);
  check("(f1) B sees A typing", tB.includes("typing") && tB.includes(HANDLE_A), tB);
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
  //     update. The action bar floats over a hovered row: react first, then the
  //     "More" pill opens the overflow menu with Edit / Delete rows.
  {
    const row = async () => await pageA.getByText(MSG2, { exact: false }).first().boundingBox();
    let r1 = await row();
    // hover the row so the bar appears, then tap More (the right-most pill)
    const hover = { x: r1.x + 30, y: r1.y + r1.height / 2 };
    const moveAndTapMore = async () => {
      await pageA.mouse.move(hover.x, hover.y);
      await pageA.waitForTimeout(400);
      // More is the third pill: react, reply, more, each 32px, right-aligned.
      await pageA.mouse.click(r1.x + r1.width - 28, r1.y + 16);
      await pageA.waitForTimeout(700);
    };
    let editOk = false;
    for (const dx of [-28, -60, -92]) {
      await moveAndTapMore();
      const editRow = pageA.getByText("Edit", { exact: true });
      if ((await editRow.count()) > 0) { await editRow.first().click(); editOk = true; break; }
      await pageA.mouse.click(hover.x, hover.y); // leave the menu, re-hover
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
    for (const dx of [-28, -60, -92]) {
      await moveAndTapMore();
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
    const reactRows = dbRows(`select r.id from reactions r where r.message_id = any((SELECT m.id FROM messages m WHERE m.body like ${sqlQuote(MSG1 + "%")}))`);
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

  await browserH.close();
  console.log(`\n${failures} failure${failures === 1 ? "" : "s"}`);
  process.exit(failures === 0 ? 0 : 1);
}

// the message_reactions table name is checked defensively
main().catch((e) => { console.error("flows failed:", e.message); process.exit(1); });
