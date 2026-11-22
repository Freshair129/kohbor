CREATE FUNCTION public.redeem_alpha_invite(p_code_hash text)
RETURNS TABLE (id uuid)
LANGUAGE sql SECURITY INVOKER SET search_path = public AS $$
  UPDATE public.alpha_invites
     SET downloads = downloads + 1, last_used_at = now()
   WHERE code_hash = p_code_hash AND NOT revoked AND downloads < max_downloads
     AND (expires_at IS NULL OR expires_at > now())
  RETURNING alpha_invites.id;
$$;
CREATE FUNCTION public.refund_alpha_invite(p_id uuid)
RETURNS void
LANGUAGE sql SECURITY INVOKER SET search_path = public AS $$
  UPDATE public.alpha_invites SET downloads = downloads - 1 WHERE id = p_id AND downloads > 0;
$$;
REVOKE ALL ON FUNCTION public.redeem_alpha_invite(text) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.refund_alpha_invite(uuid) FROM PUBLIC, anon, authenticated;
GRANT EXECUTE ON FUNCTION public.redeem_alpha_invite(text) TO service_role;
GRANT EXECUTE ON FUNCTION public.refund_alpha_invite(uuid) TO service_role;
