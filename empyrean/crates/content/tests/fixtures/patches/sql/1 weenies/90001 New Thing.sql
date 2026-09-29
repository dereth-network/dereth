/* Hand-written by unit 8b.1 in the format of ACE's WeenieSQLWriter: a weenie the base does not have. */
DELETE FROM `weenie` WHERE `class_Id` = 90001;

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (90001, 'newthing', 1, '2024-05-06 07:08:09') /* Generic */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES (90001,   1,        128) /* ItemType - Misc */
     , (90001,  19,         25) /* Value */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES (90001,  22, True ) /* Inscribable */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES (90001,   1, 'New Thing') /* Name */
     , (90001,  16, 'It''s new.
Two lines.') /* LongDesc */;
