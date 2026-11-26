// Kohbor alpha admin. POST with the caller's Supabase Auth access token and {action, ...}.
// Deploy with verify_jwt on. The caller comes only from the verified token; the database functions
// refuse anyone not listed in public.alpha_admins. Never logs the token, emails or user ids.
//   {action:"queue"}                          -> {queue:[{user_id, queue_no, email, name, joined_at, invited_at}], links:[...]}
//   {action:"set_invited", users:[uuid], invited:boolean} -> {changed:[{user_id, invited_at}]}
import { createClient } from "npm:@supabase/supabase-js@2.49.4";

const ALLOWED_ORIGIN = /^https:\/\/(kohbor-site-prototype\.vercel\.app|kohbor-site-prototype-[a-z0-9]+-pornpons-projects\.vercel\.app)$/;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

function cors(origin: string | null): Record<string, string> {
  const h: Record<string, string> = { "Vary": "Origin", "Content-Type": "application/json", "Cache-Control": "no-store" };
  if (origin && ALLOWED_ORIGIN.test(origin)) {
    h["Access-Control-Allow-Origin"] = origin;
    h["Access-Control-Allow-Methods"] = "POST, OPTIONS";
    h["Access-Control-Allow-Headers"] = "authorization, apikey, content-type";
  }
  return h;
}

function reply(status: number, body: unknown, origin: string | null) {
  return new Response(JSON.stringify(body), { status, headers: cors(origin) });
}

function failed(error: { code?: string } | null, origin: string | null) {
  if (error?.code === "42501") return reply(403, { error: "forbidden" }, origin);
  if (error?.code === "22023") return reply(400, { error: "bad_request" }, origin);
  return reply(503, { error: "unavailable" }, origin);
}

Deno.serve(async (req: Request) => {
  const origin = req.headers.get("origin");
  if (req.method === "OPTIONS") return new Response(null, { status: 204, headers: cors(origin) });
  if (req.method !== "POST") return reply(405, { error: "method" }, origin);
  const auth = req.headers.get("authorization") ?? "";
  const token = auth.startsWith("Bearer ") ? auth.slice(7) : "";
  if (!token) return reply(401, { error: "signin" }, origin);

  let body: { action?: unknown; users?: unknown; invited?: unknown };
  try { body = await req.json(); } catch { return reply(400, { error: "bad_request" }, origin); }

  const db = createClient(Deno.env.get("SUPABASE_URL")!, Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!, { auth: { persistSession: false } });
  const { data: who, error: authError } = await db.auth.getUser(token);
  if (authError || !who?.user?.id) return reply(401, { error: "signin" }, origin);
  const admin = who.user.id;

  if (body.action === "queue") {
    const q = await db.rpc("alpha_admin_queue", { p_admin: admin });
    if (q.error) return failed(q.error, origin);
    const l = await db.rpc("alpha_admin_links", { p_admin: admin });
    if (l.error) return failed(l.error, origin);
    return reply(200, { queue: q.data ?? [], links: l.data ?? [] }, origin);
  }

  if (body.action === "set_invited") {
    const users = body.users;
    if (!Array.isArray(users) || !users.length || users.length > 500 || !users.every(u => typeof u === "string" && UUID.test(u)) || typeof body.invited !== "boolean") {
      return reply(400, { error: "bad_request" }, origin);
    }
    const r = await db.rpc("alpha_admin_set_invited", { p_admin: admin, p_users: users, p_invited: body.invited });
    if (r.error) return failed(r.error, origin);
    return reply(200, { changed: r.data ?? [] }, origin);
  }

  return reply(400, { error: "bad_request" }, origin);
});
