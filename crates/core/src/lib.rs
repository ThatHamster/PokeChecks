//! PokeChecks' data types and battle logic: the Pokédex, usage stats, tournament team
//! lists, the damage calc, team building and game plans. Nothing here touches the network
//! or renders HTML, so the server and the mobile app (through WebAssembly) share it.

pub mod calc;
pub mod dex;
pub mod gameplan;
pub mod smogon;
pub mod team;
pub mod tournaments;
