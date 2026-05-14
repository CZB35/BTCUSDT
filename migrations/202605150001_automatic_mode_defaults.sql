alter table user_automation_settings
  alter column execution_mode set default 'automatic';

alter table user_automation_settings
  alter column automation_enabled set default true;

alter table user_automation_settings
  alter column require_final_confirmation set default false;

update user_automation_settings
set execution_mode = 'automatic',
    automation_enabled = true,
    require_final_confirmation = false,
    updated_at = now()
where execution_mode <> 'automatic'
   or automation_enabled = false
   or require_final_confirmation = true;

alter table user_automation_settings
  drop constraint if exists execution_mode_allowed;

alter table user_automation_settings
  add constraint execution_mode_allowed
  check (execution_mode = 'automatic');

alter table event_contract_orders
  alter column source set default 'android_automation';

alter table execution_reports
  alter column source set default 'android_automation';
