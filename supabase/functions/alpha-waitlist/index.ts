// Kohbor alpha waitlist. POST with the caller's Supabase Auth access token -> {queue_position, joined_at, invited}.
// Deploy with verify_jwt on. The user id comes only from the verified token, never from the body.
// Never logs the token, the email or the user id.
import { createClient } from "npm:@supabase/supabase-js@2.49.4";

const ALLOWED_ORIGIN = /^https:\/\/(kohbor-site-prototype\.vercel\.app|kohbor-site-prototype-[a-z0-9]+-pornpons-projects\.vercel\.app)$/;

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

Deno.serve(async (req: Request) => {
  const origin = req.headers.get("origin");
  if (req.method === "OPTIONS") return new Response(null, { status: 204, headers: cors(origin) });
  if (req.method !== "POST") return reply(405, { error: "method" }, origin);
  const auth = req.headers.get("authorization") ?? "";
  const token = auth.startsWith("Bearer ") ? auth.slice(7) : "";
  if (!token) return reply(401, { error: "signin" }, origin);

  const db = createClient(Deno.env.get("SUPABASE_URL")!, Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!, { auth: { persistSession: false } });
  const { data: who, error: authError } = await db.auth.getUser(token);
  if (authError || !who?.user?.id) return reply(401, { error: "signin" }, origin);

  const { data: rows, error } = await db.rpc("join_alpha_waitlist", { p_user_id: who.user.id });
  if (error || !Array.isArray(rows) || !rows.length) return reply(503, { error: "unavailable" }, origin);
  const row = rows[0];
  return reply(200, { queue_position: row.queue_position ?? null, joined_at: row.joined_at, invited: row.invited === true }, origin);
});
