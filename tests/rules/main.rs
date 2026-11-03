//! Tests des règles de calcul `R-xxx` (une règle = au moins un test).
//!
//! Convention (voir `tests/rules/README.md`) : un fichier par règle, nommé
//! `rNNN_titre_court.rs`, déclaré ci-dessous par `mod rNNN_titre_court;`.
//! Chaque test porte l'identifiant de sa règle dans son nom et commente ses
//! valeurs calculées à la main.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

// Modules des règles, un par fichier, dans l'ordre des identifiants.
mod r001_rounding_half_away_from_zero;
mod r002_fixed_point_quantities;
mod r003_date_domain;
mod r004_seconds_truncated;
mod r005_local_time_no_dst;
mod r006_working_unit_factors;
mod r007_elapsed_unit_factors;
mod r008_duration_sign;
mod r009_stored_minutes_options;
mod r010_percent_lag_conversion;
mod r011_time_ranges;
mod r012_standard_calendar;
mod r013_24_hours_calendar;
mod r014_night_shift_calendar;
mod r015_calendar_inheritance;
mod r016_resolution_precedence;
mod r017_calendar_exceptions;
mod r018_recurring_exceptions;
mod r019_work_weeks;
mod r020_working_between;
mod r021_add_working_forward;
mod r022_add_working_backward;
mod r023_add_working_zero;
mod r024_next_working_moment;
mod r025_previous_working_moment;
mod r026_day_uses_minutes_per_day;
mod r027_weekend_and_holiday_skipped;
mod r028_elapsed_time;
mod r029_no_working_time_and_domain;
mod r030_task_span;
mod r031_milestone_placement;
mod r032_new_project_defaults;
mod r033_date_without_time;
mod r034_effective_status_date;
mod r035_stable_topological_order;
mod r036_project_start_floor;
mod r037_finish_to_start;
mod r038_start_to_start;
mod r039_finish_to_finish;
mod r040_start_to_finish;
mod r041_lag_calendar;
mod r042_negative_lag;
mod r043_percent_lag;
mod r044_multiple_predecessors;
mod r045_project_finish;
mod r046_late_finish_without_successor;
mod r047_backward_transfer;
mod r048_total_slack;
mod r049_free_slack;
mod r050_critical_threshold;
mod r051_multiple_critical_paths;
mod r052_outline_hierarchy;
mod r053_summary_dates;
mod r054_summary_duration;
mod r055_summary_percent_complete;
mod r056_summary_predecessor_gates_subtasks;
mod r057_summary_successor;
mod r058_finish_link_into_summary;
mod r059_summary_descendant_link;
mod r060_summary_slack_and_critical;
mod r061_milestone_in_network;
mod r062_inactive_task;
mod r063_manual_task;
mod r064_manual_summary;
mod r065_link_cycle;
mod r066_task_calendar_fallback;
mod r067_elapsed_duration_task;
mod r068_project_summary;
mod r069_unschedulable_task;
mod r070_iso_duration;
mod r071_duration_format;
mod r072_link_lag_tenths;
mod r073_percent_lag_encoding;
mod r074_link_type;
mod r075_datetime_without_timezone;
mod r076_booleans_and_schema_defaults;
mod r077_working_times;
mod r078_week_days;
mod r079_base_or_derived_calendar;
mod r080_calendar_exceptions_import;
mod r081_legacy_exceptions;
mod r082_work_weeks_import;
mod r083_recorded_slack_tenths;
mod r084_project_summary_and_task_identity;
mod r085_manual_tasks_import;
mod r086_cpm_options_import;
mod r087_extras_capture;
mod r088_resources_assignments_baselines_import;
mod r089_from_finish_and_honor_import;
mod r090_constraint_date_normalization;
mod r091_as_soon_as_possible;
mod r092_as_late_as_possible;
mod r093_start_no_earlier_than;
mod r094_finish_no_earlier_than;
mod r095_start_no_later_than;
mod r096_finish_no_later_than;
mod r097_must_start_on;
mod r098_must_finish_on;
mod r099_honor_constraints_option;
mod r100_constraint_conflict_diagnostics;
mod r101_negative_slack;
mod r102_deadline;
mod r103_summary_constraints;
mod r104_manual_and_inactive_constraint;
mod r105_constraint_outside_project;
mod r106_schedule_from_finish;
mod r107_from_finish_project_start;
mod r108_from_finish_asap_and_default;
mod r109_from_finish_constraints;
mod r110_from_finish_slack_and_summaries;
mod r111_from_finish_unschedulable;
mod r112_summary_scheduled_dates;
mod r113_derived_project_calendar;
mod r120_mspdi_write_form;
mod r121_element_order_profiles;
mod r122_computed_values_written;
mod r123_extras_write;
mod r124_resources_mspdi;
mod r125_assignments_mspdi;
mod r126_baselines_mspdi;
mod r127_task_progress_write;
mod r128_calendars_write;
mod r129_manual_tasks_write;
mod r130_required_elements;
mod r131_strings_write;
mod r132_constraints_write;
mod r133_normalize;
mod r140_duration_text_parse;
mod r141_duration_text_format;
mod r142_predecessors_text;
mod r143_dates_text;
mod r144_csv_format;
mod r145_csv_import_rows;
mod r146_csv_export;
mod r150_refs_and_labels;
mod r151_uid_allocation;
mod r152_task_add;
mod r153_indent_outdent;
mod r154_task_move;
mod r155_task_delete;
mod r156_links;
mod r157_summary_computed_fields;
mod r158_entered_dates;
mod r159_constraints;
mod r160_refuse_new_errors;
mod r161_extras_invalidation;
mod r162_recorded_discarded;
mod r163_schedule_diff;
mod r164_history;
mod r165_calendar_exceptions;
mod r170_engine_assignment_duration;
mod r170_work_equals_duration_times_units;
mod r171_assignment_effective_calendar;
mod r172_calendar_intersection_union;
mod r173_assigned_leaf_placement;
mod r174_assigned_backward_constraints;
mod r175_unassigned_on_project_calendar;
mod r176_neutral_assignments;
mod r177_assignment_delay;
mod r178_leveling_delays;
mod r179_summary_assignments;
mod r180_work_contours;
mod r181_zero_units_and_bounds;
mod r182_elapsed_assigned;
mod r183_manual_inactive_assigned;
mod r184_work_rollups;
mod r185_material_cost_resources;
mod r186_overallocation;
mod r187_overallocation_scope;
mod r188_usage_by_period;
mod r189_task_type_matrix;
mod r190_add_effort_driven;
mod r191_remove_effort_driven;
mod r192_add_remove_not_effort_driven;
mod r193_fixed_work_and_summary;
mod r194_work_on_several_assignments;
mod r195_resource_commands;
mod r196_assignment_commands;
mod r197_task_update_extended;
mod r198_mspdi_resource_read;
mod r199_mspdi_resource_write;
mod r200_progress_facts;
mod r201_percent_input;
mod r202_other_progress_inputs;
mod r203_completed_leaf_pin;
mod r204_in_progress_leaf;
mod r205_in_progress_links;
mod r206_in_progress_backward;
mod r207_in_progress_from_finish;
mod r208_manual_with_progress;
mod r209_started_assignments;
mod r210_summary_progress_rollup;
mod r211_actual_work_progress;
mod r212_physical_percent;
mod r213_progress_edge_cases;
mod r214_mspdi_progress_read;
mod r215_mspdi_progress_write;
mod r216_progress_update;
mod r217_progress_reschedule;
mod r218_status_date_options;
mod r219_status_date_roles;
mod r220_baseline_values;
mod r221_baseline_scope;
mod r222_baseline_rollup;
mod r223_baseline_overwrite;
mod r224_start_finish_variance;
mod r225_duration_work_variance;
mod r226_mspdi_variances_write;
mod r227_baseline_set;
mod r228_baseline_variance;
mod r230_driver_bound;
mod r230_explain_start;
mod r231_explain_finish;
mod r232_explain_chain;
mod r233_explain_summaries;
mod r234_what_if;
mod r235_progress_views;
mod r236_late_filter;
mod r240_bar_move;
mod r241_bar_resize;
mod r242_progress_drag;
mod r243_link_drag;
mod r244_paste_rows;
mod r245_assignment_usage;
mod r246_quarter;
mod r247_set_week;
mod r248_calendar_commands;
mod r249_project_calendar;
mod r250_baseline_clear;
mod r251_task_filter;
mod r252_mark_percent;
mod r253_mode_toggle;
mod r254_manual_dates;
mod r255_active_toggle;
mod r256_external_paste;
mod r263_custom_model;
mod r264_custom_commands;
mod r265_mspdi_custom_read;
mod r266_mspdi_custom_write;

/// Vérifie que chaque fichier de `tests/rules/` suit la convention
/// `rNNN_titre.rs` et qu'il est bien déclaré comme module dans `main.rs`
/// (sinon ses tests ne tourneraient jamais).
#[test]
fn rule_files_follow_naming_convention_and_are_declared() {
    let dir = plannercraft_tests::tests_path("rules");
    let main = std::fs::read_to_string(dir.join("main.rs")).expect("lecture de rules/main.rs");
    for entry in std::fs::read_dir(&dir).expect("lecture de tests/rules") {
        let path = entry.expect("entrée de dossier").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("nom de fichier UTF-8");
        if stem == "main" {
            continue;
        }
        assert!(
            is_rule_module_name(stem),
            "{stem}.rs ne suit pas la convention rNNN_titre.rs"
        );
        let declaration = format!("\nmod {stem};");
        assert!(
            main.contains(&declaration),
            "{stem}.rs n'est pas déclaré dans rules/main.rs"
        );
    }
}

/// `r` + trois chiffres + `_` + titre en minuscules ASCII, chiffres ou `_`.
fn is_rule_module_name(stem: &str) -> bool {
    let bytes = stem.as_bytes();
    bytes.len() > 5
        && bytes[0] == b'r'
        && bytes[1..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'_'
        && bytes[5..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

#[test]
fn naming_convention_examples() {
    assert!(is_rule_module_name("r001_fs_link_lag"));
    assert!(!is_rule_module_name("r1_fs"));
    assert!(!is_rule_module_name("R001_fs"));
    assert!(!is_rule_module_name("r001"));
    assert!(!is_rule_module_name("r001_Titre"));
}
