-- Personal download links and the invite email template for the queue admin.
alter table public.alpha_download_links
  add column user_id uuid references auth.users (id) on delete set null;
create index alpha_download_links_user_id_idx on public.alpha_download_links (user_id) where user_id is not null;

-- One row: the email the admin page sends. {name}, {email} and {link} are filled in per person.
create table public.alpha_invite_template (
  id smallint primary key default 1 check (id = 1),
  subject text not null check (char_length(subject) between 1 and 200),
  body text not null check (char_length(body) between 1 and 10000),
  updated_at timestamptz not null default now()
);
alter table public.alpha_invite_template enable row level security;
revoke all on public.alpha_invite_template from anon, authenticated;

insert into public.alpha_invite_template (subject, body) values (
  'ลิงก์ดาวน์โหลดเกาะเบาะ (alpha) / Your Kohbor alpha download link',
  E'สวัสดีครับ {name}\n\nถึงคิวทดสอบเกาะเบาะของคุณแล้ว ดาวน์โหลดสำหรับ Windows ได้ที่ลิงก์นี้\n{link}\n\nลิงก์นี้สร้างให้คุณคนเดียว กรุณาอย่าส่งต่อ ถ้ามีปัญหาหรืออยากเล่าว่าใช้แล้วเป็นอย่างไร ตอบกลับอีเมลนี้ได้เลย\n\nขอบคุณที่มาช่วยทดสอบครับ\nทีมเกาะเบาะ\n\n---\n\nHi {name},\n\nYour turn in the Kohbor test queue has come. Download it for Windows here:\n{link}\n\nThis link was made for you alone, so please do not pass it on. If anything goes wrong, or you want to tell us how it went, just reply to this email.\n\nThanks for helping us test,\nThe Kohbor team\n'
);

-- The queue now also shows how often each person's own links were used.
drop function public.alpha_admin_queue(uuid);
create function public.alpha_admin_queue(p_admin uuid)
returns table (user_id uuid, queue_no bigint, email text, name text, joined_at timestamptz, invited_at timestamptz, downloads bigint)
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
           w.invited_at,
           coalesce((select sum(l.downloads) from public.alpha_download_links l where l.user_id = w.user_id), 0)::bigint
      from public.alpha_waitlist w
      join auth.users u on u.id = w.user_id
     order by w.joined_at, w.user_id;
end;
$$;

create function public.alpha_admin_get_template(p_admin uuid)
returns table (subject text, body text, updated_at timestamptz)
language plpgsql
security definer
set search_path = ''
as $$
begin
  perform public.alpha_admin_check(p_admin);
  return query select t.subject, t.body, t.updated_at from public.alpha_invite_template t where t.id = 1;
end;
$$;

create function public.alpha_admin_set_template(p_admin uuid, p_subject text, p_body text)
returns table (subject text, body text, updated_at timestamptz)
language plpgsql
security definer
set search_path = ''
as $$
begin
  perform public.alpha_admin_check(p_admin);
  if p_subject is null or p_body is null or position('{link}' in p_body) = 0 then
    raise exception 'bad_request' using errcode = '22023';
  end if;
  return query
    update public.alpha_invite_template t
       set subject = p_subject, body = p_body, updated_at = now()
     where t.id = 1
    returning t.subject, t.body, t.updated_at;
end;
$$;

-- The people to invite, in queue order, with their Google email and name.
create function public.alpha_admin_recipients(p_admin uuid, p_users uuid[])
returns table (user_id uuid, email text, name text)
language plpgsql
security definer
set search_path = ''
as $$
begin
  perform public.alpha_admin_check(p_admin);
  if p_users is null or cardinality(p_users) = 0 or cardinality(p_users) > 50 then
    raise exception 'bad_request' using errcode = '22023';
  end if;
  return query
    select w.user_id, u.email::text, coalesce(u.raw_user_meta_data->>'full_name', u.raw_user_meta_data->>'name')
      from public.alpha_waitlist w
      join auth.users u on u.id = w.user_id
     where w.user_id = any (p_users) and u.email is not null
     order by w.joined_at, w.user_id;
end;
$$;

-- Give one person a new personal link (only its SHA-256 hash is stored). Their older links are revoked.
create function public.alpha_admin_issue_link(p_admin uuid, p_user uuid, p_link_hash text)
returns table (id bigint)
language plpgsql
security definer
set search_path = ''
as $$
begin
  perform public.alpha_admin_check(p_admin);
  if not exists (select 1 from public.alpha_waitlist w where w.user_id = p_user) then
    raise exception 'bad_request' using errcode = '22023';
  end if;
  update public.alpha_download_links l set revoked = true where l.user_id = p_user and not l.revoked;
  return query
    insert into public.alpha_download_links (link_hash, label, user_id)
    values (p_link_hash, 'invite', p_user)
    returning alpha_download_links.id;
end;
$$;

-- Undo a link whose email could not be sent.
create function public.alpha_admin_revoke_link(p_admin uuid, p_link_id bigint)
returns void
language plpgsql
security definer
set search_path = ''
as $$
begin
  perform public.alpha_admin_check(p_admin);
  update public.alpha_download_links l set revoked = true where l.id = p_link_id;
end;
$$;

revoke all on function public.alpha_admin_queue(uuid) from public, anon, authenticated;
revoke all on function public.alpha_admin_get_template(uuid) from public, anon, authenticated;
revoke all on function public.alpha_admin_set_template(uuid, text, text) from public, anon, authenticated;
revoke all on function public.alpha_admin_recipients(uuid, uuid[]) from public, anon, authenticated;
revoke all on function public.alpha_admin_issue_link(uuid, uuid, text) from public, anon, authenticated;
revoke all on function public.alpha_admin_revoke_link(uuid, bigint) from public, anon, authenticated;
grant execute on function public.alpha_admin_queue(uuid) to service_role;
grant execute on function public.alpha_admin_get_template(uuid) to service_role;
grant execute on function public.alpha_admin_set_template(uuid, text, text) to service_role;
grant execute on function public.alpha_admin_recipients(uuid, uuid[]) to service_role;
grant execute on function public.alpha_admin_issue_link(uuid, uuid, text) to service_role;
grant execute on function public.alpha_admin_revoke_link(uuid, bigint) to service_role;
