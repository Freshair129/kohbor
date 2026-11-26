create table public.alpha_admins (
  user_id uuid primary key references auth.users (id) on delete cascade,
  added_at timestamptz not null default now()
);
alter table public.alpha_admins enable row level security;
revoke all on public.alpha_admins from anon, authenticated;

-- Every admin function checks the caller first and raises 'forbidden' (errcode 42501) for anyone else.
create function public.alpha_admin_check(p_admin uuid)
returns void
language plpgsql
security definer
set search_path = ''
as $$
begin
  if p_admin is null or not exists (select 1 from public.alpha_admins a where a.user_id = p_admin) then
    raise exception 'forbidden' using errcode = '42501';
  end if;
end;
$$;

-- The whole queue in sign-up order, with each person's Google email and name.
create function public.alpha_admin_queue(p_admin uuid)
returns table (user_id uuid, queue_no bigint, email text, name text, joined_at timestamptz, invited_at timestamptz)
language plpgsql
security definer
set search_path = ''
as $$
begin
  perform public.alpha_admin_check(p_admin);
  return query
    select w.user_id,
           row_number() over (order by w.joined_at, w.user_id),
           u.email::text,
           coalesce(u.raw_user_meta_data->>'full_name', u.raw_user_meta_data->>'name'),
           w.joined_at,
           w.invited_at
      from public.alpha_waitlist w
      join auth.users u on u.id = w.user_id
     order by w.joined_at, w.user_id;
end;
$$;

-- Mark people invited (p_invited true) or back in the queue (false). Returns the rows changed.
create function public.alpha_admin_set_invited(p_admin uuid, p_users uuid[], p_invited boolean)
returns table (user_id uuid, invited_at timestamptz)
language plpgsql
security definer
set search_path = ''
as $$
begin
  perform public.alpha_admin_check(p_admin);
  if p_users is null or cardinality(p_users) = 0 or cardinality(p_users) > 500 then
    raise exception 'bad_request' using errcode = '22023';
  end if;
  return query
    update public.alpha_waitlist w
       set invited_at = case when p_invited then coalesce(w.invited_at, now()) end
     where w.user_id = any (p_users)
    returning w.user_id, w.invited_at;
end;
$$;

-- Shared download links: label and use count only (the key itself is never stored).
create function public.alpha_admin_links(p_admin uuid)
returns table (id bigint, label text, revoked boolean, created_at timestamptz, downloads integer, last_used_at timestamptz)
language plpgsql
security definer
set search_path = ''
as $$
begin
  perform public.alpha_admin_check(p_admin);
  return query
    select l.id, l.label, l.revoked, l.created_at, l.downloads, l.last_used_at
      from public.alpha_download_links l
     order by l.id;
end;
$$;

revoke all on function public.alpha_admin_check(uuid) from public, anon, authenticated;
revoke all on function public.alpha_admin_queue(uuid) from public, anon, authenticated;
revoke all on function public.alpha_admin_set_invited(uuid, uuid[], boolean) from public, anon, authenticated;
revoke all on function public.alpha_admin_links(uuid) from public, anon, authenticated;
grant execute on function public.alpha_admin_check(uuid) to service_role;
grant execute on function public.alpha_admin_queue(uuid) to service_role;
grant execute on function public.alpha_admin_set_invited(uuid, uuid[], boolean) to service_role;
grant execute on function public.alpha_admin_links(uuid) to service_role;
