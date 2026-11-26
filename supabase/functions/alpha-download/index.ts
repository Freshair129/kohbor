// Kohbor alpha download gate. POST {link} or {code} -> {url} (short-lived GitHub asset link) or a fixed error.
// {link} is the secret part of a shared download link (no use limit, revocable); {code} is the older invite code.
// Never logs the link, the code, the token or the URL. verify_jwt is off: the link or code is the credential.
import { createClient } from "npm:@supabase/supabase-js@2.49.4";

const ALLOWED_ORIGIN = /^https:\/\/(kohbor-site-prototype\.vercel\.app|kohbor-site-prototype-[a-z0-9]+-pornpons-projects\.vercel\.app)$/;
const LINK_SHAPE = /^[A-Za-z0-9_-]{32,64}$/;
const CODE_SHAPE = /^KB-[0-9A-HJKMNP-TV-Z]{4}-[0-9A-HJKMNP-TV-Z]{4}-[0-9A-HJKMNP-TV-Z]{4}$/;

function cors(origin: string | null): Record<string, string> {
  const h: Record<string, string> = { "Vary": "Origin", "Content-Type": "application/json", "Cache-Control": "no-store" };
  if (origin && ALLOWED_ORIGIN.test(origin)) {
    h["Access-Control-Allow-Origin"] = origin;
    h["Access-Control-Allow-Methods"] = "POST, OPTIONS";
    h["Access-Control-Allow-Headers"] = "content-type";
  }
  return h;
}

function reply(status: number, body: unknown, origin: string | null) {
  return new Response(JSON.stringify(body), { status, headers: cors(origin) });
}

function normalize(raw: unknown): string | null {
  if (typeof raw !== "string" || raw.length > 40) return null;
  let s = raw.toUpperCase().replace(/[\s_]/g, "").replace(/O/g, "0").replace(/[IL]/g, "1");
  s = s.replace(/-/g, "");
  if (s.startsWith("KB")) s = s.slice(2);
  if (!/^[0-9A-HJKMNP-TV-Z]{12}$/.test(s)) return null;
  const code = `KB-${s.slice(0, 4)}-${s.slice(4, 8)}-${s.slice(8, 12)}`;
  return CODE_SHAPE.test(code) ? code : null;
}

async function sha256Hex(text: string): Promise<string> {
  const d = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return Array.from(new Uint8Array(d)).map((b) => b.toString(16).padStart(2, "0")).join("");
}

async function releaseAssetUrl(): Promise<string | null> {
  const token = Deno.env.get("GITHUB_RELEASES_TOKEN");
  const repo = Deno.env.get("GITHUB_RELEASES_REPO");
  if (!token || !repo || !/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repo)) return null;
  const gh = { "Authorization": `Bearer ${token}`, "X-GitHub-Api-Version": "2022-11-28", "User-Agent": "kohbor-alpha-download" };
  const rel = await fetch(`https://api.github.com/repos/${repo}/releases/latest`, { headers: { ...gh, "Accept": "application/vnd.github+json" } });
  if (!rel.ok) return null;
  const data = await rel.json();
  const assets: Array<{ name: string; url: string }> = Array.isArray(data?.assets) ? data.assets : [];
  const asset = assets.find((a) => /\.(exe|msi)$/i.test(a.name));
  if (!asset) return null;
  const file = await fetch(asset.url, { headers: { ...gh, "Accept": "application/octet-stream" }, redirect: "manual" });
  const loc = file.headers.get("location");
  await file.body?.cancel();
  return loc && loc.startsWith("https://") ? loc : null;
}

Deno.serve(async (req: Request) => {
  const origin = req.headers.get("origin");
  if (req.method === "OPTIONS") return new Response(null, { status: 204, headers: cors(origin) });
  if (req.method !== "POST") return reply(405, { error: "method" }, origin);
  let body: { code?: unknown; link?: unknown } = {};
  try { body = await req.json(); } catch { return reply(400, { error: "invalid" }, origin); }
  const db = createClient(Deno.env.get("SUPABASE_URL")!, Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!, { auth: { persistSession: false } });

  if (body.link !== undefined) {
    if (typeof body.link !== "string" || !LINK_SHAPE.test(body.link)) return reply(403, { error: "invalid" }, origin);
    const { data: rows, error } = await db.rpc("use_alpha_download_link", { p_link_hash: await sha256Hex(body.link) });
    if (error) return reply(503, { error: "unavailable" }, origin);
    if (!(Array.isArray(rows) && rows.length)) return reply(403, { error: "invalid" }, origin);
    let url: string | null = null;
    try { url = await releaseAssetUrl(); } catch { url = null; }
    return url ? reply(200, { url }, origin) : reply(503, { error: "not_ready" }, origin);
  }

  const code = normalize(body.code);
  if (!code) return reply(403, { error: "invalid" }, origin);
  const hash = await sha256Hex(code);
  const { data: rows, error } = await db.rpc("redeem_alpha_invite", { p_code_hash: hash });
  if (error) return reply(503, { error: "unavailable" }, origin);
  const id = Array.isArray(rows) && rows.length ? rows[0].id : null;
  if (!id) return reply(403, { error: "invalid" }, origin);

  let url: string | null = null;
  try { url = await releaseAssetUrl(); } catch { url = null; }
  if (!url) {
    await db.rpc("refund_alpha_invite", { p_id: id });
    return reply(503, { error: "not_ready" }, origin);
  }
  return reply(200, { url }, origin);
});
