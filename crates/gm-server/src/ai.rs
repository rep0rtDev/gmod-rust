//! HL2-style NPC brain state: conditions, engine schedules made of tasks, NPC states,
//! squads, class relationships and weapon proficiency. Ids follow the game's enums
//! (ai_condition.h, ai_task.h, ai_schedule / Enums/SCHED) so scripts can address them by
//! the same numbers and names; the native combat / movement code stays the body that the
//! schedule steps drive.
use std::collections::HashMap;

// --- NPC states (Enums/NPC_STATE)
pub const NPC_STATE_NONE: u8 = 0;
pub const NPC_STATE_IDLE: u8 = 1;
pub const NPC_STATE_ALERT: u8 = 2;
pub const NPC_STATE_COMBAT: u8 = 3;
pub const NPC_STATE_SCRIPT: u8 = 4;
pub const NPC_STATE_DEAD: u8 = 7;

// --- dispositions (Enums/D)
pub const D_ER: u8 = 0;
pub const D_HT: u8 = 1;
pub const D_FR: u8 = 2;
pub const D_LI: u8 = 3;
pub const D_NU: u8 = 4;

// --- task status (Enums/TASKSTATUS)
pub const TASKSTATUS_NEW: u8 = 0;
pub const TASKSTATUS_RUN_MOVE_AND_TASK: u8 = 1;
pub const TASKSTATUS_RUN_MOVE: u8 = 2;
pub const TASKSTATUS_RUN_TASK: u8 = 3;
pub const TASKSTATUS_COMPLETE: u8 = 4;

/// Condition ids in the engine's order (ai_condition.h).
pub const COND_NAMES: &[&str] = &[
    "COND_NONE",
    "COND_IN_PVS",
    "COND_IDLE_INTERRUPT",
    "COND_LOW_PRIMARY_AMMO",
    "COND_NO_PRIMARY_AMMO",
    "COND_NO_SECONDARY_AMMO",
    "COND_NO_WEAPON",
    "COND_SEE_HATE",
    "COND_SEE_FEAR",
    "COND_SEE_DISLIKE",
    "COND_SEE_ENEMY",
    "COND_LOST_ENEMY",
    "COND_ENEMY_WENT_NULL",
    "COND_ENEMY_OCCLUDED",
    "COND_TARGET_OCCLUDED",
    "COND_HAVE_ENEMY_LOS",
    "COND_HAVE_TARGET_LOS",
    "COND_LIGHT_DAMAGE",
    "COND_HEAVY_DAMAGE",
    "COND_PHYSICS_DAMAGE",
    "COND_REPEATED_DAMAGE",
    "COND_NPC_FREEZE",
    "COND_NPC_UNFREEZE",
    "COND_TALKER_RESPOND_TO_QUESTION",
    "COND_NO_CUSTOM_INTERRUPTS",
    "COND_BEHIND_ENEMY",
    "COND_SEE_PLAYER",
    "COND_ENEMY_FACING_ME",
    "COND_ENEMY_DEAD",
    "COND_ENEMY_UNREACHABLE",
    "COND_ENEMY_TOO_FAR",
    "COND_HEAR_DANGER",
    "COND_HEAR_THUMPER",
    "COND_HEAR_BUGBAIT",
    "COND_HEAR_COMBAT",
    "COND_HEAR_WORLD",
    "COND_HEAR_PLAYER",
    "COND_HEAR_BULLET_IMPACT",
    "COND_HEAR_PHYSICS_DANGER",
    "COND_HEAR_MOVE_AWAY",
    "COND_HEAR_SPOOKY",
    "COND_SMELL",
    "COND_TOO_CLOSE_TO_ATTACK",
    "COND_TOO_FAR_TO_ATTACK",
    "COND_NOT_FACING_ATTACK",
    "COND_WEAPON_HAS_LOS",
    "COND_WEAPON_BLOCKED_BY_FRIEND",
    "COND_WEAPON_PLAYER_IN_SPREAD",
    "COND_WEAPON_PLAYER_NEAR_TARGET",
    "COND_WEAPON_SIGHT_OCCLUDED",
    "COND_BETTER_WEAPON_AVAILABLE",
    "COND_HEALTH_ITEM_AVAILABLE",
    "COND_GIVE_WAY",
    "COND_WAY_CLEAR",
    "COND_PROVOKED",
    "COND_RECEIVED_ORDERS",
    "COND_PLAYER_ADDED_TO_SQUAD",
    "COND_PLAYER_REMOVED_FROM_SQUAD",
    "COND_PLAYER_PUSHING",
    "COND_TASK_FAILED",
    "COND_SCHEDULE_DONE",
    "COND_CAN_RANGE_ATTACK1",
    "COND_CAN_RANGE_ATTACK2",
    "COND_CAN_MELEE_ATTACK1",
    "COND_CAN_MELEE_ATTACK2",
    "COND_MOBBED_BY_ENEMIES",
    "COND_FLOATING_OFF_GROUND",
    "COND_NEW_ENEMY",
    "COND_ENEMY_WENT_NULL_",
];
pub const COND_LOW_PRIMARY_AMMO: u8 = 3;
pub const COND_NO_PRIMARY_AMMO: u8 = 4;
pub const COND_NO_WEAPON: u8 = 6;
pub const COND_SEE_HATE: u8 = 7;
pub const COND_SEE_ENEMY: u8 = 10;
pub const COND_LOST_ENEMY: u8 = 11;
pub const COND_ENEMY_WENT_NULL: u8 = 12;
pub const COND_ENEMY_OCCLUDED: u8 = 13;
pub const COND_HAVE_ENEMY_LOS: u8 = 15;
pub const COND_LIGHT_DAMAGE: u8 = 17;
pub const COND_HEAVY_DAMAGE: u8 = 18;
pub const COND_REPEATED_DAMAGE: u8 = 20;
pub const COND_SEE_PLAYER: u8 = 26;
pub const COND_ENEMY_DEAD: u8 = 28;
pub const COND_ENEMY_UNREACHABLE: u8 = 29;
pub const COND_ENEMY_TOO_FAR: u8 = 30;
pub const COND_HEAR_DANGER: u8 = 31;
pub const COND_HEAR_THUMPER: u8 = 32;
pub const COND_HEAR_BUGBAIT: u8 = 33;
pub const COND_HEAR_COMBAT: u8 = 34;
pub const COND_HEAR_WORLD: u8 = 35;
pub const COND_HEAR_PLAYER: u8 = 36;
pub const COND_HEAR_BULLET_IMPACT: u8 = 37;
pub const COND_HEAR_PHYSICS_DANGER: u8 = 38;
pub const COND_HEAR_MOVE_AWAY: u8 = 39;
pub const COND_SMELL: u8 = 41;

/// Sound-entity sound / scent types (`SOUND_*`, soundent.h).
pub const SOUND_COMBAT: u16 = 1;
pub const SOUND_WORLD: u16 = 2;
pub const SOUND_PLAYER: u16 = 4;
pub const SOUND_DANGER: u16 = 8;
pub const SOUND_BULLET_IMPACT: u16 = 16;
pub const SOUND_CARCASS: u16 = 32;
pub const SOUND_MEAT: u16 = 64;
pub const SOUND_GARBAGE: u16 = 128;
pub const SOUND_THUMPER: u16 = 256;
pub const SOUND_BUGBAIT: u16 = 512;
pub const SOUND_PHYSICS_DANGER: u16 = 1024;
pub const SOUND_MOVE_AWAY: u16 = 4096;
pub const SOUND_PLAYER_VEHICLE: u16 = 8192;
pub const SOUND_SCENTS: u16 = SOUND_CARCASS | SOUND_MEAT | SOUND_GARBAGE;

/// The condition a heard sound type raises (`CAI_BaseNPC::OnListened`): any scent is
/// COND_SMELL, a player vehicle counts as the player.
pub fn sound_condition(sound: u16) -> Option<u8> {
    Some(match sound {
        SOUND_COMBAT => COND_HEAR_COMBAT,
        SOUND_WORLD => COND_HEAR_WORLD,
        SOUND_PLAYER | SOUND_PLAYER_VEHICLE => COND_HEAR_PLAYER,
        SOUND_DANGER => COND_HEAR_DANGER,
        SOUND_BULLET_IMPACT => COND_HEAR_BULLET_IMPACT,
        SOUND_THUMPER => COND_HEAR_THUMPER,
        SOUND_BUGBAIT => COND_HEAR_BUGBAIT,
        SOUND_PHYSICS_DANGER => COND_HEAR_PHYSICS_DANGER,
        SOUND_MOVE_AWAY => COND_HEAR_MOVE_AWAY,
        s if s & SOUND_SCENTS != 0 => COND_SMELL,
        _ => return None,
    })
}

/// The sound type behind a hearing condition the native events raise directly.
pub fn condition_sound(cond: u8) -> u16 {
    match cond {
        COND_HEAR_COMBAT => SOUND_COMBAT,
        COND_HEAR_WORLD => SOUND_WORLD,
        COND_HEAR_PLAYER => SOUND_PLAYER,
        COND_HEAR_DANGER => SOUND_DANGER,
        COND_HEAR_BULLET_IMPACT => SOUND_BULLET_IMPACT,
        COND_HEAR_THUMPER => SOUND_THUMPER,
        COND_HEAR_BUGBAIT => SOUND_BUGBAIT,
        COND_HEAR_PHYSICS_DANGER => SOUND_PHYSICS_DANGER,
        COND_HEAR_MOVE_AWAY => SOUND_MOVE_AWAY,
        COND_SMELL => SOUND_SCENTS,
        _ => 0,
    }
}

/// The sound types a class listens to (`GetSoundInterests` of the HL2 classes): zombies and
/// headcrabs ignore bullet impacts, soldiers mind physics danger and move-away, antlions
/// thumpers and bugbait; no HL2 class is interested in scents, so COND_SMELL only reaches
/// scripted classes that ask for it (`ENT.SoundInterests`).
pub fn class_sound_interests(class: &str) -> u16 {
    let base = SOUND_WORLD | SOUND_COMBAT | SOUND_PLAYER | SOUND_DANGER;
    match class {
        c if class_sched_family(c) == Some("zombie")
            || class_sched_family(c) == Some("headcrab") =>
        {
            base
        }
        "npc_combine_s" => base | SOUND_PHYSICS_DANGER | SOUND_BULLET_IMPACT | SOUND_MOVE_AWAY,
        "npc_metropolice" => {
            base | SOUND_PHYSICS_DANGER
                | SOUND_BULLET_IMPACT
                | SOUND_MOVE_AWAY
                | SOUND_PLAYER_VEHICLE
        }
        "npc_antlion" | "npc_antlionguard" => {
            base | SOUND_PHYSICS_DANGER | SOUND_THUMPER | SOUND_BUGBAIT
        }
        "npc_citizen" | "npc_alyx" | "npc_barney" | "npc_kleiner" | "npc_eli" | "npc_mossman"
        | "npc_monk" | "npc_gman" | "npc_breen" | "npc_dog" | "npc_vortigaunt"
        | "npc_magnusson" | "npc_fisherman" => {
            base | SOUND_BULLET_IMPACT | SOUND_MOVE_AWAY | SOUND_PLAYER_VEHICLE
        }
        _ => base | SOUND_BULLET_IMPACT | SOUND_PLAYER_VEHICLE,
    }
}
pub const COND_TOO_CLOSE_TO_ATTACK: u8 = 42;
pub const COND_TOO_FAR_TO_ATTACK: u8 = 43;
pub const COND_NOT_FACING_ATTACK: u8 = 44;
pub const COND_WEAPON_HAS_LOS: u8 = 45;
pub const COND_TASK_FAILED: u8 = 59;
pub const COND_SCHEDULE_DONE: u8 = 60;
pub const COND_CAN_RANGE_ATTACK1: u8 = 61;
pub const COND_CAN_MELEE_ATTACK1: u8 = 63;
pub const COND_NEW_ENEMY: u8 = 67;

/// Task ids in the engine's order (ai_task.h); `ai.GetTaskID` maps the names.
pub const TASK_NAMES: &[&str] = &[
    "TASK_INVALID",
    "TASK_RESET_ACTIVITY",
    "TASK_WAIT",
    "TASK_ANNOUNCE_ATTACK",
    "TASK_WAIT_FACE_ENEMY",
    "TASK_WAIT_FACE_ENEMY_RANDOM",
    "TASK_WAIT_PVS",
    "TASK_SUGGEST_STATE",
    "TASK_TARGET_PLAYER",
    "TASK_SCRIPT_WALK_TO_TARGET",
    "TASK_SCRIPT_RUN_TO_TARGET",
    "TASK_SCRIPT_CUSTOM_MOVE_TO_TARGET",
    "TASK_MOVE_TO_TARGET_RANGE",
    "TASK_MOVE_TO_GOAL_RANGE",
    "TASK_MOVE_AWAY_PATH",
    "TASK_GET_PATH_AWAY_FROM_BEST_SOUND",
    "TASK_GET_PATH_TO_ENEMY",
    "TASK_GET_PATH_TO_ENEMY_LKP",
    "TASK_GET_CHASE_PATH_TO_ENEMY",
    "TASK_GET_PATH_TO_ENEMY_LKP_LOS",
    "TASK_GET_FLANK_RADIUS_PATH_TO_ENEMY_LOS",
    "TASK_GET_FLANK_ARC_PATH_TO_ENEMY_LOS",
    "TASK_GET_PATH_TO_ENEMY_LOS",
    "TASK_GET_PATH_TO_ENEMY_CORPSE",
    "TASK_GET_PATH_TO_PLAYER",
    "TASK_GET_PATH_TO_SAVEPOSITION",
    "TASK_GET_PATH_TO_TARGET",
    "TASK_GET_PATH_TO_RANDOM_NODE",
    "TASK_GET_PATH_TO_BESTSOUND",
    "TASK_GET_PATH_TO_BESTSCENT",
    "TASK_RUN_PATH",
    "TASK_WALK_PATH",
    "TASK_WALK_PATH_TIMED",
    "TASK_WALK_PATH_WITHIN_DIST",
    "TASK_WALK_PATH_FOR_UNITS",
    "TASK_RUN_PATH_FLEE",
    "TASK_RUN_PATH_TIMED",
    "TASK_RUN_PATH_FOR_UNITS",
    "TASK_RUN_PATH_WITHIN_DIST",
    "TASK_STRAFE_PATH",
    "TASK_CLEAR_MOVE_WAIT",
    "TASK_SMALL_FLINCH",
    "TASK_BIG_FLINCH",
    "TASK_FACE_IDEAL",
    "TASK_FACE_REASONABLE",
    "TASK_FACE_PATH",
    "TASK_FACE_PLAYER",
    "TASK_FACE_ENEMY",
    "TASK_FACE_HINTNODE",
    "TASK_FACE_TARGET",
    "TASK_FACE_LASTPOSITION",
    "TASK_FACE_SAVEPOSITION",
    "TASK_FACE_AWAY_FROM_SAVEPOSITION",
    "TASK_SET_IDEAL_YAW_TO_CURRENT",
    "TASK_RANGE_ATTACK1",
    "TASK_RANGE_ATTACK2",
    "TASK_MELEE_ATTACK1",
    "TASK_MELEE_ATTACK2",
    "TASK_RELOAD",
    "TASK_SPECIAL_ATTACK1",
    "TASK_SPECIAL_ATTACK2",
    "TASK_FIND_HINTNODE",
    "TASK_FIND_LOCK_HINTNODE",
    "TASK_CLEAR_HINTNODE",
    "TASK_LOCK_HINTNODE",
    "TASK_SOUND_ANGRY",
    "TASK_SOUND_DEATH",
    "TASK_SOUND_IDLE",
    "TASK_SOUND_WAKE",
    "TASK_SOUND_PAIN",
    "TASK_SOUND_DIE",
    "TASK_SPEAK_SENTENCE",
    "TASK_WAIT_FOR_SPEAK_FINISH",
    "TASK_SET_ACTIVITY",
    "TASK_RANDOMIZE_FRAMERATE",
    "TASK_SET_SCHEDULE",
    "TASK_SET_FAIL_SCHEDULE",
    "TASK_SET_TOLERANCE_DISTANCE",
    "TASK_SET_ROUTE_SEARCH_TIME",
    "TASK_CLEAR_FAIL_SCHEDULE",
    "TASK_PLAY_SEQUENCE",
    "TASK_PLAY_PRIVATE_SEQUENCE",
    "TASK_PLAY_PRIVATE_SEQUENCE_FACE_ENEMY",
    "TASK_PLAY_SEQUENCE_FACE_ENEMY",
    "TASK_PLAY_SEQUENCE_FACE_TARGET",
    "TASK_FIND_COVER_FROM_BEST_SOUND",
    "TASK_FIND_COVER_FROM_ENEMY",
    "TASK_FIND_LATERAL_COVER_FROM_ENEMY",
    "TASK_FIND_BACKAWAY_FROM_SAVEPOSITION",
    "TASK_FIND_NODE_COVER_FROM_ENEMY",
    "TASK_FIND_NEAR_NODE_COVER_FROM_ENEMY",
    "TASK_FIND_FAR_NODE_COVER_FROM_ENEMY",
    "TASK_FIND_COVER_FROM_ORIGIN",
    "TASK_DIE",
    "TASK_WAIT_FOR_SCRIPT",
    "TASK_PUSH_SCRIPT_ARRIVAL_ACTIVITY",
    "TASK_PLAY_SCRIPT",
    "TASK_PLAY_SCRIPT_POST_IDLE",
    "TASK_ENABLE_SCRIPT",
    "TASK_PLANT_ON_SCRIPT",
    "TASK_FACE_SCRIPT",
    "TASK_PLAY_SCENE",
    "TASK_WAIT_RANDOM",
    "TASK_WAIT_INDEFINITE",
    "TASK_STOP_MOVING",
    "TASK_TURN_LEFT",
    "TASK_TURN_RIGHT",
    "TASK_REMEMBER",
    "TASK_FORGET",
    "TASK_WAIT_FOR_MOVEMENT",
    "TASK_WAIT_FOR_MOVEMENT_STEP",
    "TASK_WAIT_UNTIL_NO_DANGER_SOUND",
    "TASK_WEAPON_FIND",
    "TASK_WEAPON_PICKUP",
    "TASK_WEAPON_RUN_PATH",
    "TASK_WEAPON_CREATE",
    "TASK_ITEM_PICKUP",
    "TASK_ITEM_RUN_PATH",
    "TASK_USE_SMALL_HULL",
    "TASK_FALL_TO_GROUND",
    "TASK_WANDER",
    "TASK_FREEZE",
    "TASK_GATHER_CONDITIONS",
    "TASK_IGNORE_OLD_ENEMIES",
    "TASK_DEBUG_BREAK",
    "TASK_ADD_HEALTH",
    "TASK_ADD_GESTURE_WAIT",
    "TASK_ADD_GESTURE",
    "TASK_GET_PATH_TO_INTERACTION_PARTNER",
];
pub const TASK_WAIT: u16 = 2;
pub const TASK_WAIT_FACE_ENEMY: u16 = 4;
pub const TASK_TARGET_PLAYER: u16 = 8;
pub const TASK_MOVE_TO_TARGET_RANGE: u16 = 12;
pub const TASK_GET_PATH_TO_ENEMY: u16 = 16;
pub const TASK_GET_PATH_TO_ENEMY_LKP: u16 = 17;
pub const TASK_GET_CHASE_PATH_TO_ENEMY: u16 = 18;
pub const TASK_GET_PATH_TO_ENEMY_LOS: u16 = 22;
pub const TASK_GET_PATH_TO_PLAYER: u16 = 24;
pub const TASK_GET_PATH_TO_SAVEPOSITION: u16 = 25;
pub const TASK_GET_PATH_TO_TARGET: u16 = 26;
pub const TASK_GET_PATH_TO_RANDOM_NODE: u16 = 27;
pub const TASK_GET_PATH_TO_BESTSOUND: u16 = 28;
pub const TASK_RUN_PATH: u16 = 30;
pub const TASK_WALK_PATH: u16 = 31;
pub const TASK_RUN_PATH_FLEE: u16 = 35;
pub const TASK_FACE_IDEAL: u16 = 43;
pub const TASK_FACE_REASONABLE: u16 = 44;
pub const TASK_FACE_PLAYER: u16 = 46;
pub const TASK_FACE_ENEMY: u16 = 47;
pub const TASK_FACE_BESTSOUND_: u16 = 44;
pub const TASK_FACE_TARGET: u16 = 49;
pub const TASK_FACE_LASTPOSITION: u16 = 50;
pub const TASK_RANGE_ATTACK1: u16 = 54;
pub const TASK_RANGE_ATTACK2: u16 = 55;
pub const TASK_MELEE_ATTACK1: u16 = 56;
pub const TASK_RELOAD: u16 = 58;
pub const TASK_SET_ACTIVITY: u16 = 73;
pub const TASK_SET_SCHEDULE: u16 = 75;
pub const TASK_SET_FAIL_SCHEDULE: u16 = 76;
pub const TASK_CLEAR_FAIL_SCHEDULE: u16 = 79;
pub const TASK_PLAY_SEQUENCE: u16 = 80;
pub const TASK_FIND_COVER_FROM_ENEMY: u16 = 86;
pub const TASK_WAIT_RANDOM: u16 = 102;
pub const TASK_WAIT_INDEFINITE: u16 = 103;
pub const TASK_STOP_MOVING: u16 = 104;
pub const TASK_WAIT_FOR_MOVEMENT: u16 = 109;
pub const TASK_WANDER: u16 = 120;

// --- schedules (Enums/SCHED)
pub const SCHED_NONE: u16 = 0;
pub const SCHED_IDLE_STAND: u16 = 1;
/// The scripted-sequence driver schedule; SetSchedule of it reads back SCHED_IDLE_STAND.
pub const SCHED_AISCRIPT: u16 = 56;
pub const SCHED_IDLE_WALK: u16 = 2;
pub const SCHED_IDLE_WANDER: u16 = 3;
pub const SCHED_WAKE_ANGRY: u16 = 4;
pub const SCHED_ALERT_FACE: u16 = 5;
pub const SCHED_ALERT_FACE_BESTSOUND: u16 = 6;
pub const SCHED_ALERT_REACT_TO_COMBAT_SOUND: u16 = 7;
pub const SCHED_ALERT_SCAN: u16 = 8;
pub const SCHED_ALERT_STAND: u16 = 9;
pub const SCHED_ALERT_WALK: u16 = 10;
pub const SCHED_INVESTIGATE_SOUND: u16 = 11;
pub const SCHED_COMBAT_FACE: u16 = 12;
pub const SCHED_COMBAT_STAND: u16 = 15;
pub const SCHED_CHASE_ENEMY: u16 = 17;
pub const SCHED_CHASE_ENEMY_FAILED: u16 = 18;
pub const SCHED_TARGET_FACE: u16 = 20;
pub const SCHED_TARGET_CHASE: u16 = 21;
pub const SCHED_SMALL_FLINCH: u16 = 22;
pub const SCHED_BIG_FLINCH: u16 = 23;
pub const SCHED_BACK_AWAY_FROM_ENEMY: u16 = 24;
pub const SCHED_TAKE_COVER_FROM_ENEMY: u16 = 27;
pub const SCHED_TAKE_COVER_FROM_BEST_SOUND: u16 = 28;
pub const SCHED_FLEE_FROM_BEST_SOUND: u16 = 29;
pub const SCHED_FAIL_TAKE_COVER: u16 = 31;
pub const SCHED_RUN_FROM_ENEMY: u16 = 32;
pub const SCHED_MOVE_TO_WEAPON_RANGE: u16 = 34;
pub const SCHED_ESTABLISH_LINE_OF_FIRE: u16 = 35;
pub const SCHED_MELEE_ATTACK1: u16 = 41;
pub const SCHED_SHOOT_ENEMY_COVER: u16 = 39;
pub const SCHED_RANGE_ATTACK1: u16 = 43;
pub const SCHED_RANGE_ATTACK2: u16 = 44;
pub const SCHED_STANDOFF: u16 = 47;
pub const SCHED_HIDE_AND_RELOAD: u16 = 50;
pub const SCHED_RELOAD: u16 = 51;
pub const SCHED_DIE: u16 = 53;
pub const SCHED_SCRIPTED_WALK: u16 = 57;
pub const SCHED_SCRIPTED_RUN: u16 = 58;
pub const SCHED_SCRIPTED_WAIT: u16 = 60;
pub const SCHED_SCRIPTED_FACE: u16 = 61;
pub const SCHED_FORCED_GO: u16 = 71;
/// Class-private schedules of the base zombie (npc_BaseZombie): the game registers them per
/// class after the shared ones, so their ids are session-dependent (see `class_schedule`).
pub const SCHED_ZOMBIE_MOVE_SWATITEM: u16 = 1001;
pub const SCHED_ZOMBIE_SWATITEM: u16 = 1002;
/// SCHED_ZOMBIE_WANDER_MEDIUM: the wander after the enemy died (TASK_WANDER 48..384 u, then
/// TASK_WAIT_PVS), measured as walk4 / Walk in NPC_STATE_ALERT until the player respawns
/// (server_npc_melee_zombie / _poisonzombie: id 146 = the family block + 7).
pub const SCHED_ZOMBIE_WANDER_MEDIUM: u16 = 1003;
/// The first id the game hands class-private schedules on a fresh server: blocks are handed
/// out in the order the classes first think (measured: a soldier spawned first reports its
/// block from 88; a zombie or antlion spawned first from 93 - the blended-animation classes
/// are preceded by a 5-id block allocated once; run14: antlion 93..138, zombie family
/// 139..166, fast zombie extension 167..168, headcrab 169..189, poison zombie extension
/// 190..191).
pub const FIRST_CLASS_SCHEDULE: u16 = 88;
pub const BLENDED_SCHED_PREFIX: u16 = 5;
/// Ids reserved for a class family without a measured block size.
pub const CLASS_SCHED_RESERVE: u16 = 12;
pub const SCHED_FORCED_GO_RUN: u16 = 72;
pub const SCHED_NPC_FREEZE: u16 = 73;
pub const SCHED_PATROL_WALK: u16 = 74;
pub const SCHED_COMBAT_PATROL: u16 = 75;
pub const SCHED_PATROL_RUN: u16 = 76;
pub const SCHED_RUN_RANDOM: u16 = 77;
/// SCHED_FALL_TO_GROUND: the body is not on the ground - here, a hull spawned overlapping a
/// physics prop while the prop is pushed clear.
pub const SCHED_FALL_TO_GROUND: u16 = 78;
pub const SCHED_VICTORY_DANCE: u16 = 19;
pub const SCHED_ESTABLISH_LINE_OF_FIRE_FALLBACK: u16 = 36;
pub const SCHED_FAIL_ESTABLISH_LINE_OF_FIRE: u16 = 38;
pub const SCHED_COWER: u16 = 40;
pub const SCHED_FAIL: u16 = 81;
pub const SCHED_FAIL_NOSTOP: u16 = 82;
/// CAI_AssaultBehavior's private schedules, mirrored while an assault goal owns the NPC: a
/// class block translates them (`class_schedule`), elsewhere they read as the shared move /
/// stand (`shared_alias`).
pub const SCHED_ASSAULT_MOVE_TO_RALLY: u16 = 1011;
pub const SCHED_ASSAULT_MOVE_TO_POINT: u16 = 1012;
pub const SCHED_ASSAULT_HOLD_RALLY: u16 = 1013;
pub const SCHED_ASSAULT_WAIT_AND_CLEAR: u16 = 1014;
/// CAI_LeadBehavior's first two schedules (its block starts with them in the game's enum).
pub const SCHED_LEAD_RETRIEVE: u16 = 1015;
pub const SCHED_LEAD_RETRIEVE_WAIT: u16 = 1016;
/// SCHED_COMBINE_SUPPRESS: the soldier's first volley, the think after it first saw the enemy
/// (server_npc_grenade_soldier*: 88 at t5, then 100 = SCHED_COMBINE_RANGE_ATTACK1).
pub const SCHED_COMBINE_SUPPRESS: u16 = 1021;
/// SCHED_COMBINE_WAIT_IN_COVER: the think after a grenade throw ends (class id +11), followed by
/// one think of SCHED_COMBAT_FACE (server_npc_grenade_soldier_wall t2246/t2253).
pub const SCHED_COMBINE_WAIT_IN_COVER: u16 = 1022;

/// The shared schedule a behaviour-private one stands for on a class without a measured block.
/// A class's TranslateSchedule onto another shared schedule, as the game reads back from
/// `SetSchedule` (server_npc_sched_translate_*): the allies take cover when a chase fails,
/// the monk and Alyx reload in place, the crabs face instead of failing cover, ...
pub fn class_shared_alias(class: &str, s: u16) -> u16 {
    let ally = matches!(
        class,
        "npc_citizen"
            | "npc_barney"
            | "npc_fisherman"
            | "npc_gman"
            | "npc_mossman"
            | "npc_monk"
            | "npc_alyx"
            | "npc_vortigaunt"
    );
    let crab = matches!(
        class,
        "npc_headcrab" | "npc_headcrab_black" | "npc_headcrab_fast"
    );
    match (class, s) {
        (_, SCHED_CHASE_ENEMY_FAILED) if ally => SCHED_TAKE_COVER_FROM_ENEMY,
        ("npc_monk" | "npc_alyx", SCHED_HIDE_AND_RELOAD) => SCHED_RELOAD,
        (_, SCHED_FAIL_TAKE_COVER) if crab => SCHED_ALERT_FACE,
        ("npc_poisonzombie", SCHED_MOVE_TO_WEAPON_RANGE) => SCHED_CHASE_ENEMY,
        ("npc_metropolice" | "npc_stalker", SCHED_FAIL_TAKE_COVER) => SCHED_RUN_RANDOM,
        ("npc_stalker", SCHED_FAIL_ESTABLISH_LINE_OF_FIRE) => SCHED_COMBAT_STAND,
        ("npc_strider", SCHED_ESTABLISH_LINE_OF_FIRE_FALLBACK) => SCHED_COMBAT_PATROL,
        ("npc_vortigaunt", SCHED_COWER) => SCHED_COMBAT_FACE,
        ("npc_alyx", SCHED_COWER) => SCHED_FAIL,
        ("npc_manhack", SCHED_WAKE_ANGRY | SCHED_COMBAT_FACE | SCHED_CHASE_ENEMY_FAILED) => {
            SCHED_CHASE_ENEMY
        }
        ("npc_headcrab_fast", SCHED_IDLE_STAND) => SCHED_PATROL_WALK,
        _ => s,
    }
}

pub fn shared_alias(s: u16) -> u16 {
    match s {
        SCHED_ASSAULT_MOVE_TO_RALLY | SCHED_ASSAULT_MOVE_TO_POINT => SCHED_FORCED_GO_RUN,
        SCHED_ASSAULT_HOLD_RALLY | SCHED_ASSAULT_WAIT_AND_CLEAR => SCHED_IDLE_STAND,
        SCHED_LEAD_RETRIEVE => SCHED_FORCED_GO,
        SCHED_LEAD_RETRIEVE_WAIT => SCHED_IDLE_STAND,
        SCHED_ZOMBIE_WANDER_MEDIUM => SCHED_IDLE_WANDER,
        SCHED_COMBINE_SUPPRESS => SCHED_RANGE_ATTACK1,
        SCHED_COMBINE_WAIT_IN_COVER => SCHED_COMBAT_FACE,
        _ => s,
    }
}

pub const SCHED_NAMES: &[(&str, u16)] = &[
    ("SCHED_NONE", 0),
    ("SCHED_IDLE_STAND", 1),
    ("SCHED_IDLE_WALK", 2),
    ("SCHED_IDLE_WANDER", 3),
    ("SCHED_WAKE_ANGRY", 4),
    ("SCHED_ALERT_FACE", 5),
    ("SCHED_ALERT_FACE_BESTSOUND", 6),
    ("SCHED_ALERT_REACT_TO_COMBAT_SOUND", 7),
    ("SCHED_ALERT_SCAN", 8),
    ("SCHED_ALERT_STAND", 9),
    ("SCHED_ALERT_WALK", 10),
    ("SCHED_INVESTIGATE_SOUND", 11),
    ("SCHED_COMBAT_FACE", 12),
    ("SCHED_COMBAT_SWEEP", 13),
    ("SCHED_FEAR_FACE", 14),
    ("SCHED_COMBAT_STAND", 15),
    ("SCHED_COMBAT_WALK", 16),
    ("SCHED_CHASE_ENEMY", 17),
    ("SCHED_CHASE_ENEMY_FAILED", 18),
    ("SCHED_VICTORY_DANCE", 19),
    ("SCHED_TARGET_FACE", 20),
    ("SCHED_TARGET_CHASE", 21),
    ("SCHED_SMALL_FLINCH", 22),
    ("SCHED_BIG_FLINCH", 23),
    ("SCHED_BACK_AWAY_FROM_ENEMY", 24),
    ("SCHED_MOVE_AWAY_FROM_ENEMY", 25),
    ("SCHED_BACK_AWAY_FROM_SAVE_POSITION", 26),
    ("SCHED_TAKE_COVER_FROM_ENEMY", 27),
    ("SCHED_TAKE_COVER_FROM_BEST_SOUND", 28),
    ("SCHED_FLEE_FROM_BEST_SOUND", 29),
    ("SCHED_TAKE_COVER_FROM_ORIGIN", 30),
    ("SCHED_FAIL_TAKE_COVER", 31),
    ("SCHED_RUN_FROM_ENEMY", 32),
    ("SCHED_RUN_FROM_ENEMY_FALLBACK", 33),
    ("SCHED_MOVE_TO_WEAPON_RANGE", 34),
    ("SCHED_ESTABLISH_LINE_OF_FIRE", 35),
    ("SCHED_ESTABLISH_LINE_OF_FIRE_FALLBACK", 36),
    ("SCHED_PRE_FAIL_ESTABLISH_LINE_OF_FIRE", 37),
    ("SCHED_FAIL_ESTABLISH_LINE_OF_FIRE", 38),
    ("SCHED_SHOOT_ENEMY_COVER", 39),
    ("SCHED_COWER", 40),
    ("SCHED_MELEE_ATTACK1", 41),
    ("SCHED_MELEE_ATTACK2", 42),
    ("SCHED_RANGE_ATTACK1", 43),
    ("SCHED_RANGE_ATTACK2", 44),
    ("SCHED_SPECIAL_ATTACK1", 45),
    ("SCHED_SPECIAL_ATTACK2", 46),
    ("SCHED_STANDOFF", 47),
    ("SCHED_ARM_WEAPON", 48),
    ("SCHED_DISARM_WEAPON", 49),
    ("SCHED_HIDE_AND_RELOAD", 50),
    ("SCHED_RELOAD", 51),
    ("SCHED_AMBUSH", 52),
    ("SCHED_DIE", 53),
    ("SCHED_DIE_RAGDOLL", 54),
    ("SCHED_WAIT_FOR_SCRIPT", 55),
    ("SCHED_AISCRIPT", 56),
    ("SCHED_SCRIPTED_WALK", 57),
    ("SCHED_SCRIPTED_RUN", 58),
    ("SCHED_SCRIPTED_CUSTOM_MOVE", 59),
    ("SCHED_SCRIPTED_WAIT", 60),
    ("SCHED_SCRIPTED_FACE", 61),
    ("SCHED_SCENE_GENERIC", 62),
    ("SCHED_NEW_WEAPON", 63),
    ("SCHED_NEW_WEAPON_CHEAT", 64),
    ("SCHED_SWITCH_TO_PENDING_WEAPON", 65),
    ("SCHED_GET_HEALTHKIT", 66),
    ("SCHED_WAIT_FOR_SPEAK_FINISH", 67),
    ("SCHED_MOVE_AWAY", 68),
    ("SCHED_MOVE_AWAY_FAIL", 69),
    ("SCHED_MOVE_AWAY_END", 70),
    ("SCHED_FORCED_GO", 71),
    ("SCHED_FORCED_GO_RUN", 72),
    ("SCHED_NPC_FREEZE", 73),
    ("SCHED_PATROL_WALK", 74),
    ("SCHED_COMBAT_PATROL", 75),
    ("SCHED_PATROL_RUN", 76),
    ("SCHED_RUN_RANDOM", 77),
    ("SCHED_FALL_TO_GROUND", 78),
    ("SCHED_DROPSHIP_DUSTOFF", 79),
    ("SCHED_FLINCH_PHYSICS", 80),
    ("SCHED_FAIL", 81),
    ("SCHED_FAIL_NOSTOP", 82),
    ("SCHED_RUN_FROM_ENEMY_MOB", 83),
    ("SCHED_DUCK_DODGE", 84),
    ("SCHED_INTERACTION_MOVE_TO_PARTNER", 85),
    ("SCHED_INTERACTION_WAIT_FOR_PARTNER", 86),
    ("SCHED_SLEEP", 87),
];

pub fn task_id(name: &str) -> Option<u16> {
    TASK_NAMES.iter().position(|n| *n == name).map(|i| i as u16)
}
pub fn cond_id(name: &str) -> Option<u8> {
    COND_NAMES.iter().position(|n| *n == name).map(|i| i as u8)
}
pub fn cond_name(id: u8) -> Option<&'static str> {
    COND_NAMES.get(id as usize).copied()
}
pub fn sched_id(name: &str) -> Option<u16> {
    SCHED_NAMES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, id)| *id)
}

/// The class family whose private schedule table a class reports through (None: only shared
/// schedules).
pub fn class_sched_family(class: &str) -> Option<&'static str> {
    match class {
        "npc_zombie"
        | "npc_zombie_torso"
        | "npc_zombine"
        | "npc_poisonzombie"
        | "npc_fastzombie"
        | "npc_fastzombie_torso" => Some("zombie"),
        "npc_headcrab" | "npc_headcrab_fast" | "npc_headcrab_black" => Some("headcrab"),
        "npc_antlion" => Some("antlion"),
        "npc_hunter" => Some("hunter"),
        "npc_combine_s" => Some("combine"),
        "npc_citizen" => Some("citizen"),
        "npc_alyx" => Some("alyx"),
        _ => None,
    }
}

/// The zombies that swat props at their enemy (the fast zombie never does, though it shares
/// the family's wake-angry think and schedule table).
pub fn class_is_zombie_body(class: &str) -> bool {
    matches!(
        class,
        "npc_zombie" | "npc_zombie_torso" | "npc_zombine" | "npc_poisonzombie"
    )
}

/// Private schedule ids a family's block takes (measured block sizes; the soldier's is a
/// lower bound - nothing has been spawned after it on a measured server).
pub fn family_sched_reserve(family: &str) -> u16 {
    match family {
        // CNPC_BaseZombie's 25 (server_npc_sched_translate_fastzombie: its own block starts at
        // family + 25); the classic zombie's own 3 follow it (measured 28 with the classic first)
        "zombie" => 25,
        "headcrab" => 21,
        "antlion" => 46,
        "combine" => 16,
        // CAI_PlayerAlly's 3 talker schedules, then the behaviours (assault first) and the
        // citizen's own; only the assault block is measured, the size is the HL2 count
        "citizen" => 60,
        // CAI_PlayerAlly + CNPC_PlayerCompanion blocks, then Alyx's own eight (her idle stand
        // reads 177 = 88 + 89 when she is the first class of the session: server_npc_spawn)
        "alyx" => 92,
        _ => CLASS_SCHED_RESERVE,
    }
}

/// Families whose first block is preceded by the blended-animation 5-id block.
pub fn family_sched_blended(family: &str) -> bool {
    matches!(family, "zombie" | "headcrab" | "antlion")
}

/// Ids a subclass adds after its family block (the fast zombie's and poison zombie's own
/// schedules).
pub fn class_sched_ext_reserve(class: &str) -> u16 {
    match class {
        "npc_zombie" | "npc_zombie_torso" | "npc_zombine" => 3,
        // range attack, unstick jump, climbing unstick jump, melee, torso melee
        "npc_fastzombie" | "npc_fastzombie_torso" => 5,
        "npc_poisonzombie" => 2,
        // the fast headcrab's own range attack (SetSchedule(RANGE_ATTACK1) reads family + 18)
        "npc_headcrab_fast" => 1,
        _ => 0,
    }
}

/// The class whose extension block a class shares: a torso is the same engine class as its
/// walker (one schedule id space).
pub fn class_sched_ext_key(class: &str) -> &str {
    match class {
        "npc_fastzombie_torso" => "npc_fastzombie",
        "npc_zombie_torso" => "npc_zombie",
        _ => class,
    }
}

/// Offset of a schedule inside a subclass's extension block.
pub fn class_ext_schedule(class: &str, shared: u16) -> Option<u16> {
    match (class, shared) {
        ("npc_fastzombie" | "npc_fastzombie_torso", SCHED_RANGE_ATTACK1) => Some(0),
        ("npc_fastzombie", SCHED_MELEE_ATTACK1) => Some(3),
        ("npc_fastzombie_torso", SCHED_MELEE_ATTACK1) => Some(4),
        ("npc_headcrab_fast", SCHED_RANGE_ATTACK1) => Some(0),
        _ => None,
    }
}

/// Offset of a schedule inside a family's private table, when the family translates it
/// (TranslateSchedule): measured on the game with `SetSchedule(shared)` / `GetCurrentSchedule`.
pub fn class_schedule(family: &str, class: &str, shared: u16) -> Option<u16> {
    match family {
        "zombie" => match shared {
            SCHED_CHASE_ENEMY => Some(0),
            SCHED_ZOMBIE_MOVE_SWATITEM => Some(1),
            SCHED_ZOMBIE_SWATITEM => Some(2),
            SCHED_ZOMBIE_WANDER_MEDIUM => Some(7),
            // SetSchedule(STANDOFF) reads the family's ninth (server_npc_sched_translate_zombie*)
            SCHED_STANDOFF => Some(9),
            SCHED_MELEE_ATTACK1 => Some(10),
            // the poison zombie reads its crab throw and range attack from the base block
            // (server_npc_sched_translate_poisonzombie: 44>+12, 43>+13)
            SCHED_RANGE_ATTACK2 if class == "npc_poisonzombie" => Some(12),
            SCHED_RANGE_ATTACK1 if class == "npc_poisonzombie" => Some(13),
            _ => None,
        },
        // SetSchedule on the game: RANGE_ATTACK1 the jump (first of the block), WAKE_ANGRY the
        // third (second for the black and fast crabs), FALL_TO_GROUND the eleventh
        "headcrab" => match shared {
            SCHED_RANGE_ATTACK1 => Some(0),
            SCHED_WAKE_ANGRY if class == "npc_headcrab" => Some(2),
            SCHED_WAKE_ANGRY => Some(1),
            SCHED_FALL_TO_GROUND => Some(10),
            _ => None,
        },
        // measured on a soldier: 88 while shooting, 92 running to reload, 97 running to a line
        // of fire, 101 the grenade toss
        "combine" => match shared {
            SCHED_COMBINE_SUPPRESS => Some(0),
            SCHED_COMBINE_WAIT_IN_COVER => Some(11),
            SCHED_RANGE_ATTACK1 => Some(12),
            SCHED_HIDE_AND_RELOAD => Some(4),
            SCHED_TAKE_COVER_FROM_ENEMY => Some(14),
            SCHED_TAKE_COVER_FROM_BEST_SOUND => Some(15),
            SCHED_ESTABLISH_LINE_OF_FIRE => Some(9),
            SCHED_RANGE_ATTACK2 => Some(13),
            // SetSchedule on the game: VICTORY_DANCE the third of the block,
            // FAIL_ESTABLISH_LINE_OF_FIRE the twenty-second
            SCHED_VICTORY_DANCE => Some(2),
            SCHED_FAIL_ESTABLISH_LINE_OF_FIRE => Some(21),
            _ => None,
        },
        // measured on a citizen under ai_goal_assault (server_npc_assault_citizen): 91 running
        // to the rally point, 96 holding it, 94 running to an assault point, 98 holding one;
        // under ai_goal_lead (server_npc_lead_citizen*): 114 retrieving the player, 115 waiting
        // sched_translate_game.txt `alyx`: SetSchedule(i) read back as the game numbers them
        // with Alyx registered first (ids here relative to FIRST_CLASS_SCHEDULE)
        "alyx" => match shared {
            43 => Some(84),
            7 => Some(85),
            12 => Some(86),
            4 => Some(87),
            SCHED_IDLE_STAND => Some(89),
            6 => Some(90),
            78 => Some(91),
            28 => Some(67),
            29 => Some(69),
            _ => None,
        },
        "citizen" => match shared {
            SCHED_ASSAULT_MOVE_TO_RALLY => Some(3),
            SCHED_ASSAULT_MOVE_TO_POINT => Some(6),
            SCHED_ASSAULT_HOLD_RALLY => Some(8),
            SCHED_ASSAULT_WAIT_AND_CLEAR => Some(10),
            SCHED_LEAD_RETRIEVE => Some(26),
            SCHED_LEAD_RETRIEVE_WAIT => Some(27),
            _ => None,
        },
        _ => None,
    }
}

/// The engine's shared schedules: the task list each runs and the conditions that interrupt
/// it (ai_default.cpp). Only the schedules the native body can carry are listed; the rest
/// resolve to the nearest one through `fallback`.
/// A schedule's task list `(task, data)` and the conditions that interrupt it.
pub type ScheduleTasks = (&'static [(u16, f32)], &'static [u8]);

pub fn schedule_tasks(id: u16) -> Option<ScheduleTasks> {
    use self::*;
    Some(match id {
        SCHED_IDLE_STAND => (
            &[
                (TASK_STOP_MOVING, 0.0),
                (TASK_SET_ACTIVITY, 1.0),
                (TASK_WAIT, 5.0),
            ],
            &[
                COND_NEW_ENEMY,
                COND_SEE_ENEMY,
                COND_LIGHT_DAMAGE,
                COND_HEAVY_DAMAGE,
                COND_HEAR_COMBAT,
                COND_HEAR_DANGER,
                COND_HEAR_PLAYER,
                COND_HEAR_BULLET_IMPACT,
                COND_SEE_HATE,
                COND_PROVOKED_,
            ],
        ),
        SCHED_IDLE_WALK | SCHED_IDLE_WANDER => (
            &[
                (TASK_WANDER, 480.0),
                (TASK_WALK_PATH, 0.0),
                (TASK_WAIT_FOR_MOVEMENT, 0.0),
                (TASK_STOP_MOVING, 0.0),
                (TASK_WAIT_RANDOM, 3.0),
            ],
            &[
                COND_NEW_ENEMY,
                COND_SEE_ENEMY,
                COND_LIGHT_DAMAGE,
                COND_HEAVY_DAMAGE,
                COND_HEAR_COMBAT,
                COND_HEAR_DANGER,
                COND_HEAR_PLAYER,
                COND_SEE_HATE,
            ],
        ),
        SCHED_WAKE_ANGRY => (&[(TASK_STOP_MOVING, 0.0), (TASK_FACE_ENEMY, 0.0)], &[]),
        SCHED_ALERT_FACE | SCHED_ALERT_FACE_BESTSOUND | SCHED_ALERT_REACT_TO_COMBAT_SOUND => (
            &[(TASK_STOP_MOVING, 0.0), (TASK_FACE_REASONABLE, 0.0)],
            &[
                COND_NEW_ENEMY,
                COND_SEE_ENEMY,
                COND_LIGHT_DAMAGE,
                COND_HEAVY_DAMAGE,
                COND_PROVOKED_,
            ],
        ),
        SCHED_ALERT_SCAN | SCHED_ALERT_STAND => (
            &[(TASK_STOP_MOVING, 0.0), (TASK_WAIT, 20.0)],
            &[
                COND_NEW_ENEMY,
                COND_SEE_ENEMY,
                COND_LIGHT_DAMAGE,
                COND_HEAVY_DAMAGE,
                COND_HEAR_COMBAT,
                COND_HEAR_DANGER,
                COND_HEAR_PLAYER,
                COND_HEAR_BULLET_IMPACT,
                COND_PROVOKED_,
            ],
        ),
        SCHED_ALERT_WALK | SCHED_PATROL_WALK => (
            &[
                (TASK_WANDER, 640.0),
                (TASK_WALK_PATH, 0.0),
                (TASK_WAIT_FOR_MOVEMENT, 0.0),
            ],
            &[
                COND_NEW_ENEMY,
                COND_SEE_ENEMY,
                COND_LIGHT_DAMAGE,
                COND_HEAVY_DAMAGE,
                COND_HEAR_DANGER,
                COND_PROVOKED_,
            ],
        ),
        SCHED_INVESTIGATE_SOUND => (
            &[
                (TASK_STOP_MOVING, 0.0),
                (TASK_GET_PATH_TO_BESTSOUND, 0.0),
                (TASK_WALK_PATH, 0.0),
                (TASK_WAIT_FOR_MOVEMENT, 0.0),
                (TASK_WAIT, 5.0),
            ],
            &[
                COND_NEW_ENEMY,
                COND_SEE_ENEMY,
                COND_LIGHT_DAMAGE,
                COND_HEAVY_DAMAGE,
                COND_HEAR_DANGER,
            ],
        ),
        SCHED_COMBAT_FACE => (
            &[
                (TASK_STOP_MOVING, 0.0),
                (TASK_FACE_ENEMY, 0.0),
                (TASK_WAIT, 1.5),
            ],
            &[
                COND_CAN_RANGE_ATTACK1,
                COND_CAN_MELEE_ATTACK1,
                COND_NEW_ENEMY,
                COND_ENEMY_DEAD,
            ],
        ),
        SCHED_COMBAT_STAND => (
            &[(TASK_STOP_MOVING, 0.0), (TASK_WAIT_INDEFINITE, 0.0)],
            &[
                COND_NEW_ENEMY,
                COND_ENEMY_DEAD,
                COND_LIGHT_DAMAGE,
                COND_HEAVY_DAMAGE,
                COND_CAN_RANGE_ATTACK1,
                COND_CAN_MELEE_ATTACK1,
            ],
        ),
        SCHED_CHASE_ENEMY | SCHED_MOVE_TO_WEAPON_RANGE | SCHED_ESTABLISH_LINE_OF_FIRE => (
            &[
                (TASK_GET_CHASE_PATH_TO_ENEMY, 300.0),
                (TASK_RUN_PATH, 0.0),
                (TASK_WAIT_FOR_MOVEMENT, 0.0),
                (TASK_FACE_ENEMY, 0.0),
            ],
            &[
                COND_NEW_ENEMY,
                COND_ENEMY_DEAD,
                COND_ENEMY_UNREACHABLE,
                COND_CAN_RANGE_ATTACK1,
                COND_CAN_MELEE_ATTACK1,
                COND_TOO_CLOSE_TO_ATTACK,
                COND_TASK_FAILED,
                COND_LOST_ENEMY,
                COND_HEAR_DANGER,
            ],
        ),
        SCHED_CHASE_ENEMY_FAILED => (
            &[
                (TASK_STOP_MOVING, 0.0),
                (TASK_WAIT, 0.2),
                (TASK_FACE_ENEMY, 0.0),
            ],
            &[COND_NEW_ENEMY, COND_CAN_RANGE_ATTACK1],
        ),
        SCHED_TARGET_FACE => (
            &[
                (TASK_STOP_MOVING, 0.0),
                (TASK_FACE_TARGET, 0.0),
                (TASK_WAIT, 0.5),
            ],
            &[
                COND_NEW_ENEMY,
                COND_LIGHT_DAMAGE,
                COND_HEAVY_DAMAGE,
                COND_PROVOKED_,
            ],
        ),
        SCHED_TARGET_CHASE => (
            &[
                (TASK_GET_PATH_TO_TARGET, 0.0),
                (TASK_MOVE_TO_TARGET_RANGE, 128.0),
                (TASK_WAIT_FOR_MOVEMENT, 0.0),
            ],
            &[
                COND_NEW_ENEMY,
                COND_LIGHT_DAMAGE,
                COND_HEAVY_DAMAGE,
                COND_PROVOKED_,
            ],
        ),
        SCHED_SMALL_FLINCH | SCHED_BIG_FLINCH => {
            (&[(TASK_STOP_MOVING, 0.0), (TASK_WAIT, 0.4)], &[])
        }
        SCHED_BACK_AWAY_FROM_ENEMY | SCHED_RUN_FROM_ENEMY | SCHED_FLEE_FROM_BEST_SOUND => (
            &[
                (TASK_STOP_MOVING, 0.0),
                (TASK_FIND_COVER_FROM_ENEMY, 0.0),
                (TASK_RUN_PATH_FLEE, 0.0),
                (TASK_WAIT_FOR_MOVEMENT, 0.0),
                (TASK_FACE_ENEMY, 0.0),
            ],
            &[COND_NEW_ENEMY, COND_ENEMY_DEAD, COND_TASK_FAILED],
        ),
        SCHED_TAKE_COVER_FROM_ENEMY | SCHED_TAKE_COVER_FROM_BEST_SOUND | SCHED_HIDE_AND_RELOAD => (
            &[
                (TASK_STOP_MOVING, 0.0),
                (TASK_FIND_COVER_FROM_ENEMY, 0.0),
                (TASK_RUN_PATH, 0.0),
                (TASK_WAIT_FOR_MOVEMENT, 0.0),
                (TASK_RELOAD, 0.0),
                (TASK_FACE_ENEMY, 0.0),
                (TASK_WAIT, 1.0),
            ],
            &[COND_NEW_ENEMY, COND_ENEMY_DEAD, COND_TASK_FAILED],
        ),
        SCHED_FAIL_TAKE_COVER => (
            &[
                (TASK_STOP_MOVING, 0.0),
                (TASK_FACE_ENEMY, 0.0),
                (TASK_WAIT, 1.0),
            ],
            &[
                COND_NEW_ENEMY,
                COND_CAN_RANGE_ATTACK1,
                COND_CAN_MELEE_ATTACK1,
            ],
        ),
        SCHED_MELEE_ATTACK1 => (
            &[
                (TASK_STOP_MOVING, 0.0),
                (TASK_FACE_ENEMY, 0.0),
                (TASK_MELEE_ATTACK1, 0.0),
            ],
            &[
                COND_NEW_ENEMY,
                COND_ENEMY_DEAD,
                COND_LIGHT_DAMAGE,
                COND_HEAVY_DAMAGE,
                COND_ENEMY_OCCLUDED,
            ],
        ),
        // the grenade toss: the native body's throw (npc_start_grenade) started by the task,
        // the schedule lasting its 63 ticks
        SCHED_RANGE_ATTACK2 => (
            &[
                (TASK_STOP_MOVING, 0.0),
                (TASK_FACE_ENEMY, 0.0),
                (TASK_RANGE_ATTACK2, 0.0),
            ],
            &[
                COND_NEW_ENEMY,
                COND_ENEMY_DEAD,
                COND_LIGHT_DAMAGE,
                COND_HEAVY_DAMAGE,
                COND_ENEMY_OCCLUDED,
                COND_NO_PRIMARY_AMMO,
                COND_HEAR_DANGER,
                COND_WEAPON_BLOCKED_BY_FRIEND_,
                COND_WEAPON_SIGHT_OCCLUDED_,
            ],
        ),
        SCHED_RANGE_ATTACK1 => (
            &[
                (TASK_STOP_MOVING, 0.0),
                (TASK_FACE_ENEMY, 0.0),
                (TASK_RANGE_ATTACK1, 0.0),
            ],
            &[
                COND_NEW_ENEMY,
                COND_ENEMY_DEAD,
                COND_LIGHT_DAMAGE,
                COND_HEAVY_DAMAGE,
                COND_ENEMY_OCCLUDED,
                COND_NO_PRIMARY_AMMO,
                COND_HEAR_DANGER,
                COND_WEAPON_BLOCKED_BY_FRIEND_,
                COND_WEAPON_SIGHT_OCCLUDED_,
            ],
        ),
        SCHED_STANDOFF => (
            &[
                (TASK_STOP_MOVING, 0.0),
                (TASK_FACE_ENEMY, 0.0),
                (TASK_WAIT_FACE_ENEMY, 2.0),
            ],
            &[
                COND_NEW_ENEMY,
                COND_ENEMY_DEAD,
                COND_CAN_RANGE_ATTACK1,
                COND_CAN_MELEE_ATTACK1,
                COND_LIGHT_DAMAGE,
                COND_HEAVY_DAMAGE,
            ],
        ),
        SCHED_RELOAD => (
            &[(TASK_STOP_MOVING, 0.0), (TASK_RELOAD, 0.0)],
            &[COND_HEAVY_DAMAGE, COND_HEAR_DANGER],
        ),
        SCHED_DIE => (&[(TASK_STOP_MOVING, 0.0)], &[]),
        SCHED_SCRIPTED_WALK => (
            &[
                (TASK_GET_PATH_TO_TARGET, 0.0),
                (TASK_WALK_PATH, 0.0),
                (TASK_WAIT_FOR_MOVEMENT, 0.0),
            ],
            &[],
        ),
        SCHED_SCRIPTED_RUN => (
            &[
                (TASK_GET_PATH_TO_TARGET, 0.0),
                (TASK_RUN_PATH, 0.0),
                (TASK_WAIT_FOR_MOVEMENT, 0.0),
            ],
            &[],
        ),
        SCHED_SCRIPTED_WAIT | SCHED_NPC_FREEZE => {
            (&[(TASK_STOP_MOVING, 0.0), (TASK_WAIT_INDEFINITE, 0.0)], &[])
        }
        SCHED_SCRIPTED_FACE => (
            &[
                (TASK_STOP_MOVING, 0.0),
                (TASK_FACE_TARGET, 0.0),
                (TASK_WAIT_INDEFINITE, 0.0),
            ],
            &[],
        ),
        SCHED_FORCED_GO => (
            &[
                (TASK_GET_PATH_TO_SAVEPOSITION, 0.0),
                (TASK_WALK_PATH, 0.0),
                (TASK_WAIT_FOR_MOVEMENT, 0.0),
                (TASK_STOP_MOVING, 0.0),
            ],
            &[],
        ),
        SCHED_FORCED_GO_RUN => (
            &[
                (TASK_GET_PATH_TO_SAVEPOSITION, 0.0),
                (TASK_RUN_PATH, 0.0),
                (TASK_WAIT_FOR_MOVEMENT, 0.0),
                (TASK_STOP_MOVING, 0.0),
            ],
            &[],
        ),
        SCHED_COMBAT_PATROL | SCHED_PATROL_RUN | SCHED_RUN_RANDOM => (
            &[
                (TASK_WANDER, 640.0),
                (TASK_RUN_PATH, 0.0),
                (TASK_WAIT_FOR_MOVEMENT, 0.0),
            ],
            &[
                COND_NEW_ENEMY,
                COND_SEE_ENEMY,
                COND_LIGHT_DAMAGE,
                COND_HEAVY_DAMAGE,
                COND_HEAR_DANGER,
            ],
        ),
        SCHED_FAIL => (
            &[
                (TASK_STOP_MOVING, 0.0),
                (TASK_SET_ACTIVITY, 1.0),
                (TASK_WAIT, 1.0),
                (TASK_WAIT_PVS_, 0.0),
            ],
            &[COND_CAN_RANGE_ATTACK1, COND_CAN_MELEE_ATTACK1],
        ),
        SCHED_FAIL_NOSTOP => (
            &[(TASK_WAIT, 1.0)],
            &[COND_CAN_RANGE_ATTACK1, COND_CAN_MELEE_ATTACK1],
        ),
        _ => return None,
    })
}
const COND_PROVOKED_: u8 = 54;
const COND_WEAPON_BLOCKED_BY_FRIEND_: u8 = 46;
const COND_WEAPON_SIGHT_OCCLUDED_: u8 = 49;
const TASK_WAIT_PVS_: u16 = 6;

/// Weapon proficiency (Enums/WEAPON_PROFICIENCY): the spread multiplier the engine's default
/// table applies (ai_basenpc.cpp: 2.5 poor, 2.0 average, 1.5 good, 1.25 very good, 1.0 perfect).
pub fn proficiency_spread(p: u8) -> f32 {
    match p {
        0 => 2.5,
        1 => 2.0,
        2 => 1.5,
        3 => 1.25,
        _ => 1.0,
    }
}

/// The proficiency an NPC class starts with (CAI_BaseNPC::CalcWeaponProficiency per class:
/// soldiers good, metrocops / citizens average, the rest average).
pub fn default_proficiency(class: &str) -> u8 {
    match class {
        "npc_combine_s" => 2,
        _ => 1,
    }
}

/// The HL2 class an NPC class name belongs to (Enums/CLASS, as the game's Classify()).
pub fn classify(class: &str) -> u8 {
    match class {
        "npc_combine_s"
        | "npc_combinedropship"
        | "npc_combinegunship"
        | "npc_helicopter"
        | "npc_strider"
        | "npc_turret_floor"
        | "npc_turret_ceiling"
        | "npc_rollermine"
        | "npc_cscanner"
        | "npc_clawscanner"
        | "npc_hunter"
        | "npc_stalker"
        | "npc_manhack"
        | "npc_sniper"
        | "npc_combine_camera"
        | "npc_turret_ground"
        | "npc_apcdriver"
        | "npc_combinegunship_" => 7, // CLASS_COMBINE
        "npc_metropolice" => 16, // CLASS_METROPOLICE
        "npc_zombie"
        | "npc_fastzombie"
        | "npc_poisonzombie"
        | "npc_zombine"
        | "npc_fastzombie_torso"
        | "npc_zombie_torso" => 20, // CLASS_ZOMBIE
        "npc_headcrab" | "npc_headcrab_fast" | "npc_headcrab_black" | "npc_headcrab_poison" => 11, // CLASS_HEADCRAB
        "npc_antlion" | "npc_antlion_worker" => 2, // CLASS_ANTLION
        "npc_antlionguard" => 2,
        "npc_barnacle" => 3, // CLASS_BARNACLE
        "npc_citizen" => 18, // CLASS_PLAYER_ALLY
        "npc_alyx" | "npc_barney" | "npc_kleiner" | "npc_eli" | "npc_mossman" | "npc_magnusson"
        | "npc_monk" | "npc_gman" | "npc_breen" | "npc_odessa" | "npc_fisherman" => 19, // CLASS_PLAYER_ALLY_VITAL
        "npc_vortigaunt" => 24, // CLASS_VORTIGAUNT
        "npc_dog" => 19,
        "npc_crow" | "npc_pigeon" | "npc_seagull" => 9, // CLASS_EARTH_FAUNA
        "npc_bullseye" => 6,                            // CLASS_BULLSEYE
        "npc_stalker_" => 23,
        _ if class.starts_with("npc_") => 17, // CLASS_NONE? -> CLASS_PLAYER_ALLY? no: CLASS_NONE
        _ => 0,
    }
}
/// The NPC state an NPC has before its first think: NONE for every class (the probe reads 0
/// and no schedule right after `spawnnpc`, allies included; with `ai_disabled 1` it stays so),
/// IDLE only for the gunship, dropship and helicopter, which think inside Spawn
/// (`server_npc_spawn`, 261107).
pub fn spawn_state(class: &str) -> u8 {
    match class {
        "npc_combinegunship" | "npc_combinedropship" | "npc_helicopter" => NPC_STATE_IDLE,
        _ => NPC_STATE_NONE,
    }
}
pub const CLASS_PLAYER: u8 = 17;
pub const CLASS_PLAYER_ALLY: u8 = 18;
pub const CLASS_PLAYER_ALLY_VITAL: u8 = 19;

/// Default disposition of class `a` toward class `b` (hl2_gamerules.cpp relationship table).
pub fn default_disposition(a: u8, b: u8) -> u8 {
    const COMBINE: u8 = 7;
    const METRO: u8 = 16;
    const ZOMBIE: u8 = 20;
    const HEADCRAB: u8 = 11;
    const ANTLION: u8 = 2;
    const BARNACLE: u8 = 3;
    const ALLY: u8 = 18;
    const ALLY_VITAL: u8 = 19;
    const VORT: u8 = 24;
    const FAUNA: u8 = 9;
    const BULLSEYE: u8 = 6;
    let human = |c: u8| matches!(c, CLASS_PLAYER | ALLY | ALLY_VITAL | VORT);
    let combine = |c: u8| matches!(c, COMBINE | METRO);
    let monster = |c: u8| matches!(c, ZOMBIE | HEADCRAB | ANTLION | BARNACLE);
    if a == b && a != 0 {
        return D_LI;
    }
    match a {
        0 => D_NU,
        BULLSEYE => D_NU,
        FAUNA => D_FR,
        CLASS_PLAYER | ALLY | ALLY_VITAL | VORT => {
            if human(b) {
                D_LI
            } else if combine(b) || monster(b) {
                D_HT
            } else {
                D_NU
            }
        }
        COMBINE | METRO => {
            if combine(b) {
                D_LI
            } else if human(b) || monster(b) {
                D_HT
            } else {
                D_NU
            }
        }
        ZOMBIE | HEADCRAB => {
            if b == ZOMBIE || b == HEADCRAB {
                D_LI
            } else if human(b) || combine(b) || b == ANTLION {
                D_HT
            } else {
                D_NU
            }
        }
        ANTLION => {
            if human(b) || combine(b) || b == ZOMBIE || b == HEADCRAB {
                D_HT
            } else {
                D_NU
            }
        }
        BARNACLE => {
            if b == BARNACLE {
                D_LI
            } else {
                D_HT
            }
        }
        _ => D_NU,
    }
}

/// An `assault_assaultpoint`: where to go, how close counts as there, how long to hold it.
#[derive(Clone, Debug, PartialEq)]
pub struct AssaultPoint {
    pub pos: glam::Vec3,
    pub tolerance: f32,
    pub timeout: f32,
    pub clear_on_contact: bool,
    pub force_crouch: bool,
    pub urgent: bool,
    /// The point's angles: the body faces them while it holds the point.
    pub yaw: f32,
}

/// `ai_goal_assault`: run to the rally point, wait for the cue (and the rally delay), then
/// run the assault point chain, holding each until its timeout or contact clears it.
#[derive(Clone, Debug, PartialEq)]
pub struct AssaultGoal {
    pub rally: glam::Vec3,
    pub rally_yaw: Option<f32>,
    pub rally_sequence: Option<String>,
    pub delay: f32,
    pub cue: bool,
    pub points: Vec<AssaultPoint>,
    pub point: usize,
    pub stage: u8,
    pub stage_since: u64,
}

pub const ASSAULT_TO_RALLY: u8 = 0;
pub const ASSAULT_AT_RALLY: u8 = 1;
pub const ASSAULT_TO_POINT: u8 = 2;
pub const ASSAULT_AT_POINT: u8 = 3;
pub const ASSAULT_DONE: u8 = 4;

/// `ai_goal_lead`: the actor retrieves the nearest player to within the lead distance and
/// waits facing them, retrieving again when they lag past it (measured on a dedicated
/// server, see `Server::ai_goal_step`); `retrieve_to` is the (point, running) of the retrieve
/// path in flight.
#[derive(Clone, Debug, PartialEq)]
pub struct LeadGoal {
    pub goal: glam::Vec3,
    pub lead_distance: f32,
    pub retrieve_distance: f32,
    pub success_distance: f32,
    pub run: bool,
    pub waiting: bool,
    pub done: bool,
    pub retrieve_to: Option<(glam::Vec3, bool)>,
}

/// An `info_node_hint` an ActBusy goal may use: where to stand and what to play there.
#[derive(Clone, Debug, PartialEq)]
pub struct ActBusyHint {
    pub pos: glam::Vec3,
    pub sequence: Option<String>,
}

/// `ai_goal_actbusy`: walk to the nearest hint in range, play its busy sequence for the busy
/// time, pick another.
#[derive(Clone, Debug, PartialEq)]
pub struct ActBusyGoal {
    pub hints: Vec<ActBusyHint>,
    pub range: f32,
    pub busy_ticks: u64,
    pub current: Option<usize>,
    pub stage: u8,
    pub stage_since: u64,
    pub next_search: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NpcGoal {
    Assault(AssaultGoal),
    Lead(LeadGoal),
    ActBusy(ActBusyGoal),
}

/// A `scripted_sequence`: how to reach the mark (`m_fMoveTo`: 0 none, 1 walk, 2 run,
/// 4 teleport, 5 turn to face), the mark, the three sequences and the stage reached
/// (SCRIPT_MOVING .. SCRIPT_DONE).
#[derive(Clone, Debug, PartialEq)]
pub struct NpcScript {
    pub move_to: u8,
    pub pos: glam::Vec3,
    pub yaw: f32,
    pub radius: f32,
    pub entry: Option<String>,
    pub play: Option<String>,
    pub post_idle: Option<String>,
    pub loop_action: bool,
    pub stage: u8,
    pub stage_since: u64,
    pub saved: Option<String>,
    pub cancelled: bool,
}

pub const SCRIPT_MOVING: u8 = 0;
pub const SCRIPT_ENTRY: u8 = 1;
pub const SCRIPT_PLAY: u8 = 2;
pub const SCRIPT_POST_IDLE: u8 = 3;
pub const SCRIPT_DONE: u8 = 4;

/// Per-NPC brain state surfaced to scripts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AiState {
    pub state: u8,
    pub schedule: Option<u16>,
    pub task_index: usize,
    pub task_status: u8,
    pub task_started: u64,
    pub task_wait_until: Option<u64>,
    pub fail_schedule: Option<u16>,
    pub conditions: u128,
    pub ignore_conditions: u128,
    pub squad: Option<String>,
    /// `hintgroup` / NPC:SetHintGroup: only hints of this group serve the NPC.
    pub hint_group: Option<String>,
    pub proficiency: u8,
    pub expression: String,
    pub behavior: Option<String>,
    /// The `func_tank` (index into the server's tanks) a FuncTank-behaviour NPC mans.
    pub func_tank: Option<usize>,
    /// The goal the behaviour works on (`ai_goal_assault` / `ai_goal_lead` / `ai_goal_actbusy`).
    pub goal: Option<NpcGoal>,
    /// A `scripted_sequence` owning the body (moves to its mark, plays entry / action / post idle).
    pub script: Option<NpcScript>,
    pub unforgettable: bool,
    pub max_route_rebuild: f32,
    pub move_delay: f32,
    pub save_position: Option<glam::Vec3>,
    /// A script owns the schedule: the native brain only moves and shoots on the schedule's say.
    pub scripted: bool,
    pub damage_ticks: Vec<u64>,
    pub schedule_done: bool,
    /// `SetSchedule` landed on another shared schedule through the class alias: it reads
    /// back as that shared id, not the class's private one (poison zombie 34 > 17).
    pub shared_alias: bool,
    pub relationships: Vec<(u32, u8, i32)>,
    pub class_relationships: Vec<(String, u8, i32)>,
    pub npc_class: Option<u8>,
    pub last_enemy: u32,
    /// `NPC:CapabilitiesAdd/Remove/Clear` replaced the class capabilities.
    pub caps_override: Option<u32>,
    /// The loudest sound heard lately (origin, COND_HEAR_* kind, tick): what INVESTIGATE_SOUND
    /// / ALERT_FACE_BESTSOUND walk to or face (the engine's best sound, kept ~3 s).
    pub last_sound: Option<(glam::Vec3, u8, u64)>,
    /// Sound types this NPC listens to when a script overrides the class's (`ENT.SoundInterests`).
    pub sound_interests: Option<u16>,
}

impl AiState {
    pub fn has(&self, c: u8) -> bool {
        c < 128 && self.conditions & (1u128 << c) != 0 && self.ignore_conditions & (1u128 << c) == 0
    }
    pub fn set(&mut self, c: u8, on: bool) {
        if c >= 128 {
            return;
        }
        if on {
            self.conditions |= 1u128 << c;
        } else {
            self.conditions &= !(1u128 << c);
        }
    }
    pub fn condition_list(&self) -> Vec<u8> {
        (0..COND_NAMES.len() as u8)
            .filter(|c| self.has(*c))
            .collect()
    }
    pub fn start(&mut self, sched: u16, now: u64) {
        self.shared_alias = false;
        self.schedule = Some(sched);
        self.task_index = 0;
        self.task_status = TASKSTATUS_NEW;
        self.task_started = now;
        self.task_wait_until = None;
        self.schedule_done = false;
        self.set(COND_SCHEDULE_DONE, false);
        self.set(COND_TASK_FAILED, false);
    }
    pub fn clear(&mut self) {
        self.schedule = None;
        self.task_index = 0;
        self.task_status = TASKSTATUS_NEW;
        self.task_wait_until = None;
        self.schedule_done = false;
    }
    pub fn current_task(&self) -> Option<(u16, f32)> {
        let (tasks, _) = schedule_tasks(self.schedule?)?;
        tasks.get(self.task_index).copied()
    }
    /// The condition that interrupts the running schedule, if any.
    pub fn interrupted_by(&self) -> Option<u8> {
        let (_, interrupts) = schedule_tasks(self.schedule?)?;
        interrupts.iter().copied().find(|c| self.has(*c))
    }
    pub fn complete_task(&mut self, now: u64) {
        self.task_status = TASKSTATUS_COMPLETE;
        self.task_index += 1;
        self.task_started = now;
        self.task_wait_until = None;
        if let Some((tasks, _)) = self.schedule.and_then(schedule_tasks) {
            if self.task_index >= tasks.len() {
                self.schedule_done = true;
                self.set(COND_SCHEDULE_DONE, true);
            } else {
                self.task_status = TASKSTATUS_NEW;
            }
        }
    }
    pub fn fail_task(&mut self) {
        self.set(COND_TASK_FAILED, true);
        self.schedule_done = true;
    }
    /// Disposition toward another entity: a per-entity override, a class override, else the
    /// game's table (players are CLASS_PLAYER).
    pub fn disposition(
        &self,
        own_class: &str,
        other: u32,
        other_class: Option<&str>,
        is_player: bool,
    ) -> u8 {
        if let Some((_, d, _)) = self
            .relationships
            .iter()
            .rev()
            .find(|(e, _, _)| *e == other)
        {
            return *d;
        }
        let oc = if is_player {
            "player".to_string()
        } else {
            other_class.unwrap_or("").to_string()
        };
        if let Some((_, d, _)) = self
            .class_relationships
            .iter()
            .rev()
            .find(|(c, _, _)| *c == oc)
        {
            return *d;
        }
        let a = self.npc_class.unwrap_or_else(|| classify(own_class));
        let b = if is_player {
            CLASS_PLAYER
        } else {
            classify(other_class.unwrap_or(""))
        };
        default_disposition(a, b)
    }
}

/// Squads: name -> ordered members (the first alive member leads).
#[derive(Default, Debug, Clone, PartialEq)]
pub struct Squads {
    pub members: HashMap<String, Vec<u32>>,
}
impl Squads {
    pub fn join(&mut self, name: &str, id: u32) {
        let m = self.members.entry(name.to_string()).or_default();
        if !m.contains(&id) {
            m.push(id);
        }
    }
    pub fn leave(&mut self, id: u32) {
        for m in self.members.values_mut() {
            m.retain(|x| *x != id);
        }
        self.members.retain(|_, m| !m.is_empty());
    }
    pub fn of(&self, id: u32) -> Option<&str> {
        self.members
            .iter()
            .find(|(_, m)| m.contains(&id))
            .map(|(n, _)| n.as_str())
    }
    pub fn leader(&self, name: &str) -> Option<u32> {
        self.members.get(name).and_then(|m| m.first().copied())
    }
    pub fn list(&self, name: &str) -> Vec<u32> {
        self.members.get(name).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_follow_the_engine_tables() {
        assert_eq!(task_id("TASK_WAIT"), Some(TASK_WAIT));
        assert_eq!(
            task_id("TASK_WAIT_FOR_MOVEMENT"),
            Some(TASK_WAIT_FOR_MOVEMENT)
        );
        assert_eq!(cond_id("COND_SEE_ENEMY"), Some(COND_SEE_ENEMY));
        assert_eq!(cond_name(COND_TASK_FAILED), Some("COND_TASK_FAILED"));
        assert_eq!(sched_id("SCHED_CHASE_ENEMY"), Some(SCHED_CHASE_ENEMY));
        assert_eq!(
            default_disposition(classify("npc_combine_s"), CLASS_PLAYER),
            D_HT
        );
        assert_eq!(
            default_disposition(classify("npc_citizen"), CLASS_PLAYER),
            D_LI
        );
        assert_eq!(
            default_disposition(classify("npc_zombie"), classify("npc_combine_s")),
            D_HT
        );
        assert_eq!(
            default_disposition(classify("npc_zombie"), classify("npc_headcrab")),
            D_LI
        );
    }
    #[test]
    fn schedule_steps_and_interrupts() {
        let mut s = AiState::default();
        s.start(SCHED_IDLE_STAND, 10);
        assert_eq!(s.current_task(), Some((TASK_STOP_MOVING, 0.0)));
        s.complete_task(11);
        s.complete_task(12);
        s.complete_task(13);
        assert!(s.schedule_done && s.has(COND_SCHEDULE_DONE));
        s.start(SCHED_CHASE_ENEMY, 20);
        s.set(COND_ENEMY_DEAD, true);
        assert_eq!(s.interrupted_by(), Some(COND_ENEMY_DEAD));
        s.ignore_conditions |= 1u128 << COND_ENEMY_DEAD;
        assert_eq!(s.interrupted_by(), None);
    }
}
