-- Snapshot each owner's former global presentation before removing it below.
-- Updating all icons deliberately includes applications retained in the trash.
-- JSON null icons can acquire presentation fields without inventing icon content.
-- Leave malformed/non-object legacy payloads untouched rather than losing data.
WITH settings AS (
 SELECT user_id,
  CASE WHEN json_valid(panel_json) THEN
   CASE WHEN json_type(panel_json)='object' THEN panel_json ELSE '{}' END
  ELSE '{}' END AS panel
 FROM user_config
), icons AS (
 SELECT item_icon.id,
  CASE WHEN json_type(icon_json)='object' THEN icon_json ELSE '{}' END AS icon,
  COALESCE(settings.panel,'{}') AS panel
 FROM item_icon LEFT JOIN settings ON settings.user_id=item_icon.user_id
 WHERE CASE WHEN json_valid(icon_json) THEN json_type(icon_json) IN ('object','null') ELSE 0 END
)
UPDATE item_icon SET icon_json=(
 SELECT json_set(icon,
  '$.style',COALESCE(json_extract(icon,'$.style'),json_extract(panel,'$.iconStyle'),1),
  '$.textColor',COALESCE(json_extract(icon,'$.textColor'),json_extract(panel,'$.iconTextColor'),'#ffffff'),
  '$.hideDescription',json(COALESCE(NULLIF(icon -> '$.hideDescription','null'),NULLIF(panel -> '$.iconTextInfoHideDescription','null'),'false')),
  '$.hideTitle',json(COALESCE(NULLIF(icon -> '$.hideTitle','null'),NULLIF(panel -> '$.iconTextIconHideTitle','null'),'false'))
 ) FROM icons WHERE icons.id=item_icon.id
) WHERE id IN (SELECT id FROM icons);

-- System-monitor cards share the former global layout but already own their
-- colors. Keep every card and extendParam field, adding only a missing style.
WITH settings AS (
 SELECT user_id,
  CASE WHEN json_valid(panel_json) THEN
   CASE WHEN json_type(panel_json)='object' THEN panel_json ELSE '{}' END
  ELSE '{}' END AS panel
 FROM user_config
), monitors AS (
 SELECT module_config.id,json_set(value_json,'$.list',json((
  SELECT json_group_array(json(CASE
   WHEN card.type='object' AND COALESCE(json_type(card.value,'$.extendParam'),'null') IN ('object','null') THEN
    json_set(card.value,'$.extendParam',json_set(
     CASE WHEN json_type(card.value,'$.extendParam')='object' THEN json_extract(card.value,'$.extendParam') ELSE '{}' END,
     '$.style',COALESCE(json_extract(card.value,'$.extendParam.style'),json_extract(settings.panel,'$.iconStyle'),1)
    ))
   ELSE value_json -> ('$.list[' || card.key || ']')
  END)) FROM json_each(value_json,'$.list') AS card
 ))) AS migrated
 FROM module_config LEFT JOIN settings ON settings.user_id=module_config.user_id
 WHERE name='systemMonitor'
  AND CASE WHEN json_valid(value_json) THEN json_type(value_json,'$.list')='array' ELSE 0 END
)
UPDATE module_config SET value_json=(SELECT migrated FROM monitors WHERE monitors.id=module_config.id),
 revision=revision+1 WHERE id IN (SELECT id FROM monitors);

-- Invalidate pre-migration settings revisions so an old page cannot restore the
-- removed global fields with a stale compare-and-swap write.
UPDATE user_config SET panel_json=json_remove(panel_json,
 '$.iconStyle','$.iconTextColor','$.iconTextInfoHideDescription','$.iconTextIconHideTitle'
), revision=revision+1
WHERE CASE WHEN json_valid(panel_json) THEN json_type(panel_json)='object' ELSE 0 END;
