/* Darkenfowl (look 2): an AC2 Darkenfowl as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:darkenfowl2}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:darkenfowl2}}, 'ac2-darkenfowl2', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:darkenfowl2}},   1,         16) /* ItemType - Creature */
     , ({{wcid:darkenfowl2}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:darkenfowl2}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:darkenfowl2}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:darkenfowl2}},  16,          1) /* ItemUseable - No */
     , ({{wcid:darkenfowl2}},  25,          5) /* Level */
     , ({{wcid:darkenfowl2}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:darkenfowl2}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:darkenfowl2}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:darkenfowl2}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:darkenfowl2}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:darkenfowl2}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:darkenfowl2}},   1, True ) /* Stuck */
     , ({{wcid:darkenfowl2}},  11, False) /* IgnoreCollisions */
     , ({{wcid:darkenfowl2}},  12, True ) /* ReportCollisions */
     , ({{wcid:darkenfowl2}},  13, False) /* Ethereal */
     , ({{wcid:darkenfowl2}},  14, True ) /* GravityStatus */
     , ({{wcid:darkenfowl2}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:darkenfowl2}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:darkenfowl2}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:darkenfowl2}},   3,    0.5) /* HealthRate */
     , ({{wcid:darkenfowl2}},   4,      5) /* StaminaRate */
     , ({{wcid:darkenfowl2}},   5,      1) /* ManaRate */
     , ({{wcid:darkenfowl2}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:darkenfowl2}},  34,      1) /* PowerupTime */
     , ({{wcid:darkenfowl2}},  36,      1) /* ChargeSpeed */
     , ({{wcid:darkenfowl2}},  39,    1.2) /* DefaultScale */
     , ({{wcid:darkenfowl2}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:darkenfowl2}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:darkenfowl2}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:darkenfowl2}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:darkenfowl2}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:darkenfowl2}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:darkenfowl2}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:darkenfowl2}},  64,      1) /* ResistSlash */
     , ({{wcid:darkenfowl2}},  65,      1) /* ResistPierce */
     , ({{wcid:darkenfowl2}},  66,      1) /* ResistBludgeon */
     , ({{wcid:darkenfowl2}},  67,      1) /* ResistFire */
     , ({{wcid:darkenfowl2}},  68,      1) /* ResistCold */
     , ({{wcid:darkenfowl2}},  69,      1) /* ResistAcid */
     , ({{wcid:darkenfowl2}},  70,      1) /* ResistElectric */
     , ({{wcid:darkenfowl2}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:darkenfowl2}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:darkenfowl2}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:darkenfowl2}},  74,      1) /* ResistManaDrain */
     , ({{wcid:darkenfowl2}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:darkenfowl2}}, 1, 'Darkenfowl (look 2)') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:darkenfowl2}},   1, 0x0200E300) /* Setup */
     , ({{wcid:darkenfowl2}},   2, 0x0900E300) /* MotionTable */
     , ({{wcid:darkenfowl2}},   4, 0x3000E300) /* CombatTable */
     , ({{wcid:darkenfowl2}},   8, 0x06001035) /* Icon */;

/* This look's pictures, in place of the body's own. */
INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)
VALUES ({{wcid:darkenfowl2}}, 0, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 0, 0x0500E301, 0x0500E303)
     , ({{wcid:darkenfowl2}}, 1, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 1, 0x0500E301, 0x0500E303)
     , ({{wcid:darkenfowl2}}, 2, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 3, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 4, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 5, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 6, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 7, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 8, 0x0500E301, 0x0500E303)
     , ({{wcid:darkenfowl2}}, 9, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 10, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 10, 0x0500E301, 0x0500E303)
     , ({{wcid:darkenfowl2}}, 11, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 11, 0x0500E301, 0x0500E303)
     , ({{wcid:darkenfowl2}}, 12, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 13, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 13, 0x0500E301, 0x0500E303)
     , ({{wcid:darkenfowl2}}, 14, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 14, 0x0500E301, 0x0500E303)
     , ({{wcid:darkenfowl2}}, 15, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 16, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 17, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 18, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 19, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 20, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 21, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 22, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 23, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 24, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 25, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 26, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 27, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 28, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 29, 0x0500E300, 0x0500E302)
     , ({{wcid:darkenfowl2}}, 30, 0x0500E300, 0x0500E302);

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:darkenfowl2}}, 1, 40, 0, 0)
     , ({{wcid:darkenfowl2}}, 2, 40, 0, 0)
     , ({{wcid:darkenfowl2}}, 3, 40, 0, 0)
     , ({{wcid:darkenfowl2}}, 4, 40, 0, 0)
     , ({{wcid:darkenfowl2}}, 5, 10, 0, 0)
     , ({{wcid:darkenfowl2}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:darkenfowl2}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:darkenfowl2}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:darkenfowl2}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:darkenfowl2}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:darkenfowl2}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:darkenfowl2}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:darkenfowl2}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:darkenfowl2}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:darkenfowl2}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:darkenfowl2}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:darkenfowl2}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:darkenfowl2}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:darkenfowl2}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:darkenfowl2}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:darkenfowl2}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
