// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Auth/Account.cs
//! `Account`: a row of the `auth` database (Entity Framework model).

use empyrean_common::dotnet::datetime::DotNetDateTime;

// ACE: Account
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Account {
    // ACE: Account.AccountId
    pub account_id: u32,
    // ACE: Account.AccountName
    pub account_name: String,
    // ACE: Account.PasswordHash
    pub password_hash: String,
    // ACE: Account.PasswordSalt
    pub password_salt: String,
    // ACE: Account.AccessLevel
    pub access_level: u32,
    // ACE: Account.EmailAddress
    pub email_address: Option<String>,
    // ACE: Account.CreateTime
    pub create_time: DotNetDateTime,
    // ACE: Account.CreateIP
    pub create_ip: Option<Vec<u8>>,
    // ACE: Account.LastLoginTime
    pub last_login_time: Option<DotNetDateTime>,
    // ACE: Account.LastLoginIP
    pub last_login_ip: Option<Vec<u8>>,
    // ACE: Account.TotalTimesLoggedIn
    pub total_times_logged_in: u32,
    // ACE: Account.BannedTime
    pub banned_time: Option<DotNetDateTime>,
    // ACE: Account.BannedByAccountId
    pub banned_by_account_id: Option<u32>,
    // ACE: Account.BanExpireTime
    pub ban_expire_time: Option<DotNetDateTime>,
    // ACE: Account.BanReason
    pub ban_reason: Option<String>,
}
