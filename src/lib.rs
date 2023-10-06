//! Other Resources
//!
//! - Source code - [github.com/spartan-holochain-counsel/rust-hc-crud-caps](https://github.com/spartan-holochain-counsel/rust-hc-crud-caps/)
//! - Cargo package - [crates.io/crates/hc_crud_caps](https://crates.io/crates/hc_crud_caps)
//!

mod errors;
mod entities;
mod utils;

use std::convert::TryFrom;
use hdk::prelude::*;

pub use entities::{ Entity, EmptyEntity, EntryModel };
pub use errors::{ UtilsResult, UtilsError };
pub use utils::{
    now, find_latest_link, path_from_collection,
    trace_action_history, to_entry_type,
};


#[derive(Debug, Serialize, Deserialize)]
pub struct GetEntityInput {
    pub id: ActionHash,
}

impl Into<GetEntityInput> for ActionHash {
    fn into(self) -> GetEntityInput {
	GetEntityInput {
	    id: self,
	}
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateEntityInput<T> {
    pub base: ActionHash,
    pub properties: T,
}

impl<T> UpdateEntityInput<T> {
    pub fn new(base: ActionHash, properties: T) -> Self {
	UpdateEntityInput { base, properties }
    }
}


/// Get the entity ID for any given entity EntryHash
pub fn get_origin_address(addr: &ActionHash) -> UtilsResult<ActionHash> {
    let chain = trace_action_history( addr )?;

    // The starting 'addr' will always be in the chain so it is safe to unwrap.
    Ok( chain.last().unwrap().0.to_owned() )
}

/// Get the record for any given EntryHash
pub fn fetch_record(action: &ActionHash) -> UtilsResult<(ActionHash, Record)> {
    let record = get( action.to_owned(), GetOptions::latest() )?
	.ok_or( UtilsError::ActionNotFoundError(action.to_owned(), Some("".to_string())) )?;

    Ok( (record.action_address().to_owned(), record) )
}

/// Finds and returns the Action with the earliest timestamp from a list
pub fn find_earliest_action(updates: Vec<SignedHashed<Action>>) -> Option<SignedHashed<Action>> {
    if updates.len() == 0 {
	None
    }
    else {
	Some( updates.iter()
            .fold( None, |acc, sh| {
		let ts = sh.action().timestamp();
		match acc {
		    None => Some( (ts, sh.to_owned()) ),
		    Some(current) => {
			Some(match current.0 < ts {
			    true => current,
			    false => (ts, sh.to_owned()),
			})
		    }
		}
	    }).unwrap().1 )
    }
}


/// Follow the trail of (earliest) updates and return the full Action path.
pub fn follow_updates(hash: &ActionHash, trace: Option<Vec<ActionHash>>) -> UtilsResult<Vec<ActionHash>> {
    let mut history = trace.unwrap_or( Vec::new() );
    history.push( hash.to_owned() );

    let details = get_details( hash.to_owned(), GetOptions::latest() )?
	.ok_or( UtilsError::ActionNotFoundError(hash.to_owned(), Some("".to_string())) )?;
    let updates = match details {
	Details::Record(details) => details.updates,
	Details::Entry(details) => details.updates,
    };

    Ok( match find_earliest_action( updates ) {
	None => history,
	Some(next_update) => follow_updates( next_update.action_address(), Some(history) )?,
    })
}

/// Get the latest Record for any given entity ID
pub fn fetch_record_latest(id: &ActionHash) -> UtilsResult<(ActionHash, Record)> {
    let (action_hash, first_record) = fetch_record( id )?;

    match first_record.action() {
	Action::Create(_) => (),
	_ => Err(UtilsError::NotOriginEntryError(action_hash.to_owned()))?,
    }

    let updates = follow_updates( &action_hash, None )?;
    let latest_action_hash = updates.last().unwrap();
    let record = get( latest_action_hash.to_owned(), GetOptions::latest() )?
	.ok_or( UtilsError::ActionNotFoundError(action_hash.to_owned(), Some("".to_string())) )?;

    Ok( (action_hash, record) )
}



/// Create a new entity
pub fn create_entity<T,I,E>(entry: &T) -> UtilsResult<Entity<T>>
where
    ScopedEntryDefIndex: for<'a> TryFrom<&'a I, Error = WasmError>,
    EntryVisibility: for<'a> From<&'a I>,
    Entry: TryFrom<I, Error = E>,
    Entry: TryFrom<T, Error = E>,
    WasmError: From<E>,
    T: Clone + EntryModel<I>,
{
    let entry_hash = hash_entry( entry.to_owned() )?;
    let action_hash = create_entry( entry.to_input() )?;

    Ok(Entity {
	id: action_hash.to_owned(),
	address: entry_hash,
	action: action_hash,
	ctype: entry.get_type(),
	content: entry.to_owned(),
    })
}

/// Get an entity by its ID
pub fn get_entity<I,ET>(id: &ActionHash) -> UtilsResult<Entity<I>>
where
    I: TryFrom<Record, Error = WasmError> + Clone + EntryModel<ET>,
    Entry: TryFrom<I, Error = WasmError>,
    ScopedEntryDefIndex: for<'a> TryFrom<&'a ET, Error = WasmError>,
{
    let (_, record) = fetch_record_latest( id )?;
    let to_type_input = record.to_owned();
    let address = record
	.action()
	.entry_hash()
	.ok_or(UtilsError::ActionNotFoundError(id.to_owned(), None))?;

    let content : I = to_entry_type( to_type_input )?;

    Ok(Entity {
	id: id.to_owned(),
	action: record.action_address().to_owned(),
	address: address.to_owned(),
	ctype: content.get_type(),
	content: content,
    })
}

/// Update an entity
pub fn update_entity<T,I,F,E>(addr: &ActionHash, callback: F) -> UtilsResult<Entity<T>>
where
    ScopedEntryDefIndex: for<'a> TryFrom<&'a I, Error = WasmError>,
    Entry: TryFrom<I, Error = E>,
    Entry: TryFrom<T, Error = E>,
    WasmError: From<E>,
    T: TryFrom<Record, Error = WasmError>,
    T: Clone + EntryModel<I>,
    F: FnOnce(T, Record) -> UtilsResult<T>,
{
    // TODO: provide automatic check that the given address is the latest one or an optional flag
    // to indicate the intension to branch from an older update.
    let id = get_origin_address( &addr )?;
    let record = get( addr.to_owned(), GetOptions::latest() )?
	.ok_or( UtilsError::ActionNotFoundError(addr.to_owned(), Some("Given origin for update is not found".to_string())) )?;

    let current : T = to_entry_type( record.clone() )?;
    let updated_entry = callback( current, record.clone() )?;

    let entry_hash = hash_entry( updated_entry.to_owned() )?;
    let action_hash = update_entry( addr.to_owned(), updated_entry.to_input() )?;

    Ok(Entity {
	id: id,
	action: action_hash,
	address: entry_hash,
	ctype: updated_entry.get_type(),
	content: updated_entry,
    })
}

/// Delete an entity
pub fn delete_entity<T,ET>(id: &ActionHash) -> UtilsResult<ActionHash>
where
    T: TryFrom<Record, Error = WasmError> + Clone + EntryModel<ET>,
    Entry: TryFrom<T, Error = WasmError>,
    ScopedEntryDefIndex: for<'a> TryFrom<&'a ET, Error = WasmError>,
{
    let (action_hash, record) = fetch_record( id )?;
    let _ : T = to_entry_type( record )?;
    let delete_hash = delete_entry( action_hash )?;

    Ok( delete_hash )
}


/// Get multiple entities for a given base and link tag filter
pub fn get_entities<T,LT,ET,B>(id: &B, link_type: LT, tag: Option<Vec<u8>>) -> UtilsResult<Vec<Entity<T>>>
where
    T: TryFrom<Record, Error = WasmError> + Clone + EntryModel<ET>,
    B: Into<AnyLinkableHash> + Clone,
    LT: LinkTypeFilterExt,
    Entry: TryFrom<T, Error = WasmError>,
    ScopedEntryDefIndex: for<'a> TryFrom<&'a ET, Error = WasmError>,
{
    let links_result = get_links( GetLinksInput {
        base_address: id.to_owned().into(),
        link_type: link_type.try_into_filter()?,
        tag_prefix: tag.map( |tag| LinkTag::new( tag ) ),
        after: None,
        before: None,
        author: None
    });
    debug!("get_entities: {:?}", links_result );
    let links = links_result?;

    let list = links.into_iter()
	.filter_map(|link| {
	    link.target.into_action_hash()
		.and_then( |target| get_entity( &target ).ok() )
	})
	.collect();

    Ok(list)
}
