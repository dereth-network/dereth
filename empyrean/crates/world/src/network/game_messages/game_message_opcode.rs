// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/GameMessageOpcode.cs
//! Port of `Source/ACE.Server/Network/GameMessages/GameMessageOpcode.cs`.

// ACE: GameMessageOpcode
/// ACE enum `GameMessageOpcode`, underlying `int`. A newtype so that duplicate values (ACE declares
/// aliases) and values outside the declared set both survive, as a C# enum does.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Debug)]
#[repr(transparent)]
pub struct GameMessageOpcode(pub u32);

#[allow(non_upper_case_globals)]
impl GameMessageOpcode {
    pub const None: Self = Self(0x0000);
    pub const InventoryRemoveObject: Self = Self(0x0024);
    pub const SetStackSize: Self = Self(0x0197);
    pub const PlayerKilled: Self = Self(0x019E);
    pub const EmoteText: Self = Self(0x01E0);
    pub const SoulEmote: Self = Self(0x01E2);
    pub const HearSpeech: Self = Self(0x02BB);
    pub const HearRangedSpeech: Self = Self(0x02BC);
    pub const PrivateUpdatePropertyInt: Self = Self(0x02CD);
    pub const PublicUpdatePropertyInt: Self = Self(0x02CE);
    pub const PrivateUpdatePropertyInt64: Self = Self(0x02CF);
    pub const PublicUpdatePropertyInt64: Self = Self(0x02D0);
    pub const PrivateUpdatePropertyBool: Self = Self(0x02D1);
    pub const PublicUpdatePropertyBool: Self = Self(0x02D2);
    pub const PrivateUpdatePropertyFloat: Self = Self(0x02D3);
    pub const PublicUpdatePropertyFloat: Self = Self(0x02D4);
    pub const PrivateUpdatePropertyString: Self = Self(0x02D5);
    pub const PublicUpdatePropertyString: Self = Self(0x02D6);
    pub const PrivateUpdatePropertyDataID: Self = Self(0x02D7);
    pub const PublicUpdatePropertyDataID: Self = Self(0x02D8);
    pub const PrivateUpdatePropertyInstanceID: Self = Self(0x02D9);
    pub const PublicUpdateInstanceId: Self = Self(0x02DA);
    pub const PrivateUpdatePosition: Self = Self(0x02DB);
    pub const PublicUpdatePosition: Self = Self(0x02DC);
    pub const PrivateUpdateSkill: Self = Self(0x02DD);
    pub const PublicUpdateSkill: Self = Self(0x02DE);
    pub const PrivateUpdateSkillLevel: Self = Self(0x02DF);
    pub const PublicUpdateSkillLevel: Self = Self(0x02E0);
    pub const PrivateUpdateAttribute: Self = Self(0x02E3);
    pub const PublicUpdateAttribute: Self = Self(0x02E4);
    pub const PrivateUpdateVital: Self = Self(0x02E7);
    pub const PublicUpdateVital: Self = Self(0x02E8);
    pub const PrivateUpdateAttribute2ndLevel: Self = Self(0x02E9);
    pub const AdminEnvirons: Self = Self(0xEA60);
    pub const PositionAndMovement: Self = Self(0xF619);
    pub const ObjDescEvent: Self = Self(0xF625);
    pub const CharacterCreateResponse: Self = Self(0xF643);
    pub const CharacterRestoreResponse: Self = Self(0xF643);
    pub const CharacterLogOff: Self = Self(0xF653);
    pub const CharacterDelete: Self = Self(0xF655);
    pub const CharacterCreate: Self = Self(0xF656);
    pub const CharacterEnterWorld: Self = Self(0xF657);
    pub const CharacterList: Self = Self(0xF658);
    pub const CharacterError: Self = Self(0xF659);
    pub const ForceObjectDescSend: Self = Self(0xF6EA);
    pub const ObjectCreate: Self = Self(0xF745);
    pub const PlayerCreate: Self = Self(0xF746);
    pub const ObjectDelete: Self = Self(0xF747);
    pub const UpdatePosition: Self = Self(0xF748);
    pub const ParentEvent: Self = Self(0xF749);
    pub const PickupEvent: Self = Self(0xF74A);
    pub const SetState: Self = Self(0xF74B);
    pub const MovementEvent: Self = Self(0xF74C);
    pub const Motion: Self = Self(0xF74C);
    pub const VectorUpdate: Self = Self(0xF74E);
    pub const Sound: Self = Self(0xF750);
    pub const PlayerTeleport: Self = Self(0xF751);
    pub const AutonomousPosition: Self = Self(0xF753);
    pub const PlayScriptId: Self = Self(0xF754);
    pub const PlayEffect: Self = Self(0xF755);
    pub const GameEvent: Self = Self(0xF7B0);
    pub const GameAction: Self = Self(0xF7B1);
    pub const AccountBanned: Self = Self(0xF7C1);
    pub const CharacterEnterWorldRequest: Self = Self(0xF7C8);
    pub const GetServerVersion: Self = Self(0xF7CC);
    pub const FriendsOld: Self = Self(0xF7CD);
    pub const CharacterRestore: Self = Self(0xF7D9);
    pub const AccountBoot: Self = Self(0xF7DC);
    pub const UpdateObject: Self = Self(0xF7DB);
    pub const TurbineChat: Self = Self(0xF7DE);
    pub const CharacterEnterWorldServerReady: Self = Self(0xF7DF);
    pub const ServerMessage: Self = Self(0xF7E0);
    pub const ServerName: Self = Self(0xF7E1);
    pub const DDD_DataMessage: Self = Self(0xF7E2);
    pub const DDD_RequestDataMessage: Self = Self(0xF7E3);
    pub const DDD_ErrorMessage: Self = Self(0xF7E4);
    pub const DDD_Interrogation: Self = Self(0xF7E5);
    pub const DDD_InterrogationResponse: Self = Self(0xF7E6);
    pub const DDD_BeginDDD: Self = Self(0xF7E7);
    pub const DDD_BeginPullDDD: Self = Self(0xF7E8);
    pub const DDD_IterationData: Self = Self(0xF7E9);
    pub const DDD_EndDDD: Self = Self(0xF7EA);
    // Not ACE: the overlay extension's manifest (V437).
    pub const DDD_OverlayManifest: Self = Self(0xF7EC);
}
