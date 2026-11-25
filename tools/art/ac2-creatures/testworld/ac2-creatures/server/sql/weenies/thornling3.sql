/* Thornling (look 3): an AC2 Thornling as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:thornling3}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:thornling3}}, 'ac2-thornling3', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:thornling3}},   1,         16) /* ItemType - Creature */
     , ({{wcid:thornling3}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:thornling3}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:thornling3}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:thornling3}},  16,          1) /* ItemUseable - No */
     , ({{wcid:thornling3}},  25,          5) /* Level */
     , ({{wcid:thornling3}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:thornling3}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:thornling3}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:thornling3}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:thornling3}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:thornling3}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:thornling3}},   1, True ) /* Stuck */
     , ({{wcid:thornling3}},  11, False) /* IgnoreCollisions */
     , ({{wcid:thornling3}},  12, True ) /* ReportCollisions */
     , ({{wcid:thornling3}},  13, False) /* Ethereal */
     , ({{wcid:thornling3}},  14, True ) /* GravityStatus */
     , ({{wcid:thornling3}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:thornling3}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:thornling3}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:thornling3}},   3,    0.5) /* HealthRate */
     , ({{wcid:thornling3}},   4,      5) /* StaminaRate */
     , ({{wcid:thornling3}},   5,      1) /* ManaRate */
     , ({{wcid:thornling3}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:thornling3}},  34,      1) /* PowerupTime */
     , ({{wcid:thornling3}},  36,      1) /* ChargeSpeed */
     , ({{wcid:thornling3}},  39,    1.0) /* DefaultScale */
     , ({{wcid:thornling3}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:thornling3}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:thornling3}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:thornling3}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:thornling3}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:thornling3}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:thornling3}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:thornling3}},  64,      1) /* ResistSlash */
     , ({{wcid:thornling3}},  65,      1) /* ResistPierce */
     , ({{wcid:thornling3}},  66,      1) /* ResistBludgeon */
     , ({{wcid:thornling3}},  67,      1) /* ResistFire */
     , ({{wcid:thornling3}},  68,      1) /* ResistCold */
     , ({{wcid:thornling3}},  69,      1) /* ResistAcid */
     , ({{wcid:thornling3}},  70,      1) /* ResistElectric */
     , ({{wcid:thornling3}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:thornling3}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:thornling3}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:thornling3}},  74,      1) /* ResistManaDrain */
     , ({{wcid:thornling3}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:thornling3}}, 1, 'Thornling (look 3)') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:thornling3}},   1, 0x0200EA00) /* Setup */
     , ({{wcid:thornling3}},   2, 0x0900EA00) /* MotionTable */
     , ({{wcid:thornling3}},   4, 0x3000EA00) /* CombatTable */
     , ({{wcid:thornling3}},   8, 0x06001035) /* Icon */;

/* This look's pictures, in place of the body's own. */
INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)
VALUES ({{wcid:thornling3}}, 0, 0x0500EA00, 0x0500EA04)
     , ({{wcid:thornling3}}, 0, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 1, 0x0500EA00, 0x0500EA04)
     , ({{wcid:thornling3}}, 1, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 2, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 3, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 4, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 5, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 6, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 7, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 8, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 9, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 10, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 11, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 12, 0x0500EA00, 0x0500EA04)
     , ({{wcid:thornling3}}, 12, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 13, 0x0500EA00, 0x0500EA04)
     , ({{wcid:thornling3}}, 13, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 14, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 16, 0x0500EA00, 0x0500EA04)
     , ({{wcid:thornling3}}, 16, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 17, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 18, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 19, 0x0500EA00, 0x0500EA04)
     , ({{wcid:thornling3}}, 19, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 20, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 21, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 22, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 23, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 24, 0x0500EA00, 0x0500EA04)
     , ({{wcid:thornling3}}, 24, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling3}}, 25, 0x0500EA00, 0x0500EA04)
     , ({{wcid:thornling3}}, 26, 0x0500EA00, 0x0500EA04)
     , ({{wcid:thornling3}}, 27, 0x0500EA00, 0x0500EA04)
     , ({{wcid:thornling3}}, 28, 0x0500EA00, 0x0500EA04);

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:thornling3}}, 1, 40, 0, 0)
     , ({{wcid:thornling3}}, 2, 40, 0, 0)
     , ({{wcid:thornling3}}, 3, 40, 0, 0)
     , ({{wcid:thornling3}}, 4, 40, 0, 0)
     , ({{wcid:thornling3}}, 5, 10, 0, 0)
     , ({{wcid:thornling3}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:thornling3}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:thornling3}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:thornling3}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:thornling3}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:thornling3}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:thornling3}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:thornling3}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:thornling3}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:thornling3}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:thornling3}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:thornling3}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:thornling3}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:thornling3}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:thornling3}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:thornling3}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
