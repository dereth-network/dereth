INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (300, 'scrolltest', 34, '2021-11-01 00:00:00') /* Scroll */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES (300,   1, 'Scroll of Test') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES (300,   1, 0x02000002) /* Setup */
     , (300,  28,       1234) /* Spell - UNKNOWN__GUESSEDNAME1234 */;
