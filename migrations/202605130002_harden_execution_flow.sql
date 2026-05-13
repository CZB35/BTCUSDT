alter table execution_reports
  alter column report_status set default 'pending_review';

update execution_reports
set report_status = 'pending_review'
where report_status = 'pending';

alter table event_contract_orders
  alter column reservation_id set not null;

create index if not exists signal_reservations_user_signal_idx
  on signal_reservations(user_id, signal_id);

alter table execution_reports
  drop constraint if exists execution_reports_status_allowed;

alter table execution_reports
  add constraint execution_reports_status_allowed
  check (report_status in ('pending_review', 'settled', 'rejected', 'review_required'));

alter table signal_reservations
  drop constraint if exists signal_reservations_status_allowed;

alter table signal_reservations
  add constraint signal_reservations_status_allowed
  check (status in ('reserved', 'order_submitted', 'settled', 'released', 'expired'));
