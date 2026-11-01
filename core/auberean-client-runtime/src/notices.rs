//! The notices the scripts send the interface's panels: each read as its own native reads it, and
//! heard by the panels its sender names.
//!
//! A notice takes a fixed number of words below the receiver (its native's own count, which every
//! call site in the library pushes) and answers 1 below them. Nearly every notice is about an
//! object: its first word names the engine object, the panels listening on that object hear it, and
//! the rest are the notice's own words (an object's id as two words, a reference to a native
//! object, a number). The interface's own three (portal space, the portal's scene, opening the
//! examination panel) are heard by every panel: none names an object, and every word is the
//! notice's.

/// Who hears a notice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hearers {
    /// The panels listening on the object the first word names.
    TheObjects,
    /// Every panel.
    Everyone,
}

/// How a notice is read: its native, who hears it, and how many words it takes below the receiver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reading {
    pub native: u32,
    pub hearers: Hearers,
    pub slots: i64,
}

/// A notice of a package's function: its native id.
const fn id(package: u32, number: u32) -> u32 {
    package << 16 | number
}

const fn about(package: u32, number: u32, slots: i64) -> Reading {
    Reading {
        native: id(package, number),
        hearers: Hearers::TheObjects,
        slots,
    }
}

const fn everyone(package: u32, number: u32, slots: i64) -> Reading {
    Reading {
        native: id(package, number),
        hearers: Hearers::Everyone,
        slots,
    }
}

/// How a notice native is read; `None` for a native that is not a notice.
#[must_use]
pub fn reading(native: u32) -> Option<Reading> {
    NOTICES.iter().find(|n| n.native == native).copied()
}

/// Every notice the client binds.
pub const NOTICES: &[Reading] = &[
    // D2-00815, D2-00153
    about(0x84, 0x01, 2),
    // D2-00814, D2-00152
    about(0x84, 0x02, 4),
    // D2-00813, D2-00151
    about(0x84, 0x03, 4),
    // D2-00816, D2-00154
    about(0x84, 0x04, 4),
    // D2-00845, D2-00187
    about(0x85, 0x01, 6),
    // D2-00832, D2-00174
    about(0x85, 0x02, 1),
    // D2-00830, D2-00171
    about(0x85, 0x03, 3),
    // D2-00839, D2-00181
    about(0x85, 0x04, 2),
    // D2-18958, D2-00184
    about(0x85, 0x05, 5),
    // D2-00841, D2-00183
    about(0x85, 0x06, 2),
    // D2-00838, D2-00180
    about(0x85, 0x07, 1),
    // D2-00831, D2-00173
    about(0x85, 0x08, 2),
    // D2-00825, D2-00165
    about(0x85, 0x09, 1),
    // D2-00819, D2-00157
    about(0x85, 0x0A, 1),
    // D2-00840, D2-00182
    about(0x85, 0x0B, 2),
    // D2-00821, D2-00160
    about(0x85, 0x0C, 3),
    // D2-00833, D2-00175
    about(0x85, 0x0D, 2),
    // D2-00844, D2-00186
    about(0x85, 0x0E, 4),
    // D2-00843, D2-00185
    about(0x85, 0x0F, 5),
    // D2-00828, D2-00168
    about(0x85, 0x10, 2),
    // D2-00835, D2-00177
    about(0x85, 0x11, 2),
    // D2-00834, D2-00176
    about(0x85, 0x12, 6),
    // D2-00824, D2-00163
    about(0x85, 0x13, 2),
    // D2-00817, D2-00155
    about(0x85, 0x14, 7),
    // D2-00820, D2-00159
    about(0x85, 0x15, 2),
    // D2-00826, D2-00166
    about(0x85, 0x16, 4),
    // D2-00818, D2-00156
    about(0x85, 0x17, 6),
    // D2-00827, D2-00167
    about(0x85, 0x18, 3),
    // D2-00837, D2-00179
    about(0x85, 0x19, 4),
    // D2-00836, D2-00178
    about(0x85, 0x1A, 4),
    // D2-00823, D2-00162
    about(0x85, 0x1B, 3),
    // D2-00829, D2-00169
    about(0x85, 0x1C, 2),
    // D2-00822, D2-00161
    about(0x85, 0x1D, 2),
    // D2-00850, D2-00192
    about(0x86, 0x01, 2),
    // D2-00847, D2-00189
    about(0x86, 0x02, 3),
    // D2-00848, D2-00190
    about(0x86, 0x03, 2),
    // D2-00846, D2-00188
    about(0x86, 0x04, 2),
    // D2-00849, D2-00191
    about(0x86, 0x05, 3),
    // D2-00851, D2-00193
    about(0x87, 0x01, 3),
    // D2-00852, D2-00194
    about(0x87, 0x02, 2),
    // D2-00853, D2-00195
    about(0x87, 0x04, 1),
    // D2-00858, D2-00200
    about(0x88, 0x01, 1),
    // D2-00854, D2-00196
    about(0x88, 0x02, 7),
    // D2-00855, D2-00197
    about(0x88, 0x03, 3),
    // D2-00857, D2-00199
    about(0x88, 0x04, 4),
    // D2-00856, D2-00198
    about(0x88, 0x05, 2),
    // D2-00859, D2-00201
    about(0x88, 0x06, 2),
    // D2-00861, D2-00203
    about(0x89, 0x01, 2),
    // D2-00860, D2-00202
    about(0x89, 0x02, 2),
    // D2-00873, D2-00215
    about(0x8A, 0x01, 2),
    // D2-00863, D2-00205
    about(0x8A, 0x02, 1),
    // D2-00866, D2-00208
    about(0x8A, 0x03, 4),
    // D2-00862, D2-00204
    about(0x8A, 0x04, 3),
    // D2-00869, D2-00211
    about(0x8A, 0x05, 4),
    // D2-00865, D2-00207
    about(0x8A, 0x06, 5),
    // D2-00864, D2-00206
    about(0x8A, 0x07, 3),
    // D2-00868, D2-00210
    about(0x8A, 0x08, 5),
    // D2-00870, D2-00212
    about(0x8A, 0x09, 4),
    // D2-00872, D2-00214
    about(0x8A, 0x0A, 5),
    // D2-00871, D2-00213
    about(0x8A, 0x0B, 4),
    // D2-00874, D2-00216
    about(0x8A, 0x0C, 3),
    // D2-00867, D2-00209
    about(0x8A, 0x0D, 4),
    // D2-00906, D2-00234
    about(0x8B, 0x01, 5),
    // D2-00905, D2-00233
    about(0x8B, 0x02, 7),
    // D2-00902, D2-00230
    about(0x8B, 0x03, 4),
    // D2-00903, D2-00231
    about(0x8B, 0x04, 1),
    // D2-00904, D2-00232
    about(0x8B, 0x05, 1),
    // D2-00907, D2-00235
    about(0x8C, 0x01, 2),
    // D2-00908, D2-00236
    about(0x8C, 0x02, 2),
    // D2-00909, D2-00245
    about(0x8D, 0x01, 2),
    // D2-00910, D2-00260
    about(0x8E, 0x01, 3),
    // D2-00911, D2-00261
    about(0x8E, 0x02, 3),
    // D2-00912, D2-00272
    about(0x8E, 0x03, 2),
    // D2-00913, D2-00273
    about(0x8F, 0x01, 1),
    // D2-00914, D2-00274
    about(0x8F, 0x02, 2),
    // D2-00933, D2-00304
    about(0x90, 0x01, 6),
    // D2-00932, D2-00303
    about(0x90, 0x02, 1),
    // D2-00929, D2-00300
    about(0x90, 0x03, 4),
    // D2-00928, D2-00299
    about(0x90, 0x04, 3),
    // D2-00930, D2-00301
    about(0x90, 0x05, 3),
    // D2-00934, D2-00305
    about(0x90, 0x06, 1),
    // D2-00927, D2-00298
    about(0x90, 0x07, 1),
    // D2-00931, D2-00302
    about(0x90, 0x08, 6),
    // D2-00935, D2-00306
    about(0x90, 0x09, 5),
    // D2-00937, D2-00320
    everyone(0x91, 0x01, 1),
    // D2-00936, D2-00319
    everyone(0x91, 0x02, 0),
    // D2-00938, D2-00327
    everyone(0x91, 0x03, 2),
    // D2-00939, D2-00328
    about(0x92, 0x01, 2),
    // D2-00941, D2-00330
    about(0x92, 0x02, 6),
    // D2-00940, D2-00329
    about(0x92, 0x03, 7),
    // D2-00901, D2-00229
    about(0x1AD2, 0x01, 2),
    // D2-00887, D2-00218
    about(0x1AD2, 0x02, 2),
    // D2-00889, D2-00220
    about(0x1AD2, 0x03, 2),
    // D2-00888, D2-00219
    about(0x1AD2, 0x04, 2),
    // D2-00890, D2-00221
    about(0x1AD2, 0x05, 2),
    // D2-00926, D2-00297
    about(0x1AD3, 0x01, 3),
    // D2-00916, D2-00278
    about(0x1AD3, 0x02, 1),
    // D2-00923, D2-00291
    about(0x1AD3, 0x03, 2),
    // D2-00921, D2-00289
    about(0x1AD3, 0x04, 2),
    // D2-00920, D2-00282
    about(0x1AD3, 0x05, 4),
    // D2-00918, D2-00280
    about(0x1AD3, 0x06, 1),
    // D2-00915, D2-00276
    about(0x1AD3, 0x07, 5),
    // D2-00922, D2-00290
    about(0x1AD3, 0x08, 2),
    // D2-00924, D2-00288
    about(0x1AD3, 0x09, 2),
    // D2-00917, D2-00279
    about(0x1AD3, 0x0A, 1),
    // D2-00919, D2-00281
    about(0x1AD3, 0x0B, 2),
];
