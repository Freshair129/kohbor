
create table public.alpha_waitlist (
  user_id uuid primary key references auth.users (id) on delete cascade,
  joined_at timestamptz not null default now(),
  invited_at timestamptz,
  note text
);
alter table public.alpha_waitlist enable row level security;
revoke all on public.alpha_waitlist from anon, authenticated;

create table public.alpha_download_links (
  id bigint generated always as identity primary key,
  link_hash text not null unique check (link_hash ~ '^[0-9a-f]{64}$'),
  label text,
  revoked boolean not null default false,
  created_at timestamptz not null default now(),
  downloads integer not null default 0,
  last_used_at timestamptz
);
alter table public.alpha_download_links enable row level security;
revoke all on public.alpha_download_links from anon, authenticated;

-- Join the queue once; return the place among people not yet invited (null once invited).
create function public.join_alpha_waitlist(p_user_id uuid)
returns table (queue_position bigint, joined_at timestamptz, invited boolean)
language plpgsql
security definer
set search_path = ''
as $$
declare
  me public.alpha_waitlist%rowtype;
begin
  insert into public.alpha_waitlist (user_id) values (p_user_id) on conflict (user_id) do nothing;
  select * into me from public.alpha_waitlist w where w.user_id = p_user_id;
  return query
    select case when me.invited_at is null then
             (select count(*) from public.alpha_waitlist w where w.invited_at is null and w.joined_at <= me.joined_at)
           end,
           me.joined_at,
           me.invited_at is not null;
end;
$$;

-- Count a download through a shared link; returns the link id, or no row when unknown or revoked.
create function public.use_alpha_download_link(p_link_hash text)
returns table (id bigint)
language sql
security definer
set search_path = ''
as $$
  update public.alpha_download_links l
     set downloads = l.downloads + 1, last_used_at = now()
   where l.link_hash = p_link_hash and not l.revoked
  returning l.id;
$$;

revoke all on function public.join_alpha_waitlist(uuid) from public, anon, authenticated;
revoke all on function public.use_alpha_download_link(text) from public, anon, authenticated;
grant execute on function public.join_alpha_waitlist(uuid) to service_role;
grant execute on function public.use_alpha_download_link(text) to service_role;
