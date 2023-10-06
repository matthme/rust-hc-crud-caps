use std::convert::TryFrom;
use hdk::prelude::*;
use crate::errors::{
    UtilsResult, UtilsError,
};


/// Identifies a struct as an Entity model type
pub trait EntryModel<T>
where
    ScopedEntryDefIndex: for<'a> TryFrom<&'a T, Error = WasmError>,
{
    fn name() -> &'static str;
    fn get_type(&self) -> String;
    fn to_input(&self) -> T;
}

#[macro_export]
macro_rules! entry_model {
    ($types:ident::$name:ident( $entry:ident ) ) => {
	entry_model!( $types::$name, $entry );
    };
    ($types:ident::$name:ident, $entry:ident ) => {
	impl hc_crud::EntryModel<$types> for $entry {
	    fn name() -> &'static str { stringify!($name) }
	    fn get_type(&self) -> String {
		$entry::name().to_lowercase()
	    }
	    fn to_input(&self) -> $types {
		$types::$name(self.clone())
	    }
	}
    };
}


/// The context and content of a specific entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity<T> {
    /// The address of the original create action
    pub id: ActionHash,

    /// The create/update action of the current entry
    pub action: ActionHash,

    /// The address of the current entry
    pub address: EntryHash,

    #[serde(rename = "type")]
    /// An identifier for the content's type and structure
    pub ctype: String,

    /// The entity's current value
    pub content: T,
}

impl<T> Entity<T> {

    /// Link this entity to the given base with a specific tag.  Shortcut for [`hdk::prelude::create_link`]
    pub fn link_from<L,E,B>(&self, base: &B, link_type: L, tag_input: Option<Vec<u8>>) -> UtilsResult<ActionHash>
    where
	B: Into<AnyLinkableHash> + Clone,
	ScopedLinkType: TryFrom<L, Error = E>,
        WasmError: From<E>,
    {
        Ok( match tag_input {
	    None => create_link( base.to_owned(), self.id.to_owned(), link_type, () )?,
	    Some(input) => create_link( base.to_owned(), self.id.to_owned(), link_type, input )?,
	})
    }

    /// Link the given target to this entity with a specific tag.  Shortcut for [`hdk::prelude::create_link`]
    pub fn link_to<L,E,B>(&self, target: &B, link_type: L, tag_input: Option<Vec<u8>>) -> UtilsResult<ActionHash>
    where
	B: Into<AnyLinkableHash> + Clone,
	ScopedLinkType: TryFrom<L, Error = E>,
        WasmError: From<E>,
    {
        Ok( match tag_input {
	    None => create_link( self.id.to_owned(), target.to_owned(), link_type, () )?,
	    Some(input) => create_link( self.id.to_owned(), target.to_owned(), link_type, input )?,
	})
    }

    /// Delete an existing link from the 'current_base' and create a new link from the 'new_base'
    pub fn move_link_from<LT,E,B>(&self, link_type: LT, tag_input: Option<Vec<u8>>, current_base: &B, new_base: &B) -> UtilsResult<ActionHash>
    where
	B: Into<AnyLinkableHash> + Clone,
	LT: LinkTypeFilterExt + Clone + std::fmt::Debug,
        ScopedLinkType: TryFrom<LT, Error = E>,
        WasmError: From<E>,
    {
	let tag_filter = tag_input.to_owned().map( |tag| LinkTag::new( tag ) );
	let all_links = get_links( GetLinksInput {
		base_address: current_base.clone().into(),
		link_type: link_type.to_owned().try_into_filter()?,
		tag_prefix: tag_filter.to_owned(),
		after: None,
		before: None,
		author: None,
	})?;

	if let Some(current_link) = all_links.into_iter().find(|link| {
	    link.target == self.id.to_owned().into()
	}) {
            delete_link( current_link.create_link_hash )?;
	}
	else {
	    Err(UtilsError::UnexpectedState(format!("Aborting 'move_from_link' because existing link was not found")))?;
	};

	let new_links = get_links( GetLinksInput {
		base_address: new_base.clone().into(),
		link_type: link_type.clone().try_into_filter()?,
		tag_prefix: tag_filter.to_owned(),
		after: None,
		before: None,
		author: None
	})?;

	if let Some(existing_link) = new_links.into_iter().find(|link| {
	    link.target == self.id.to_owned().into()
	}) {
            Ok( existing_link.create_link_hash )
	}
	else {
            self.link_from( new_base, link_type, tag_input )
	}
    }
}


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Empty {}

/// A general use entity definition for deserializing any entity input when the content is not
/// relevant.
pub type EmptyEntity = Entity<Empty>;



#[cfg(test)]
pub mod tests {
    use super::*;
    use rand::Rng;

    #[test]
    fn entity_test() {
	let bytes = rand::thread_rng().gen::<[u8; 32]>();
	let ahash = holo_hash::ActionHash::from_raw_32( bytes.to_vec() );
	let ehash = holo_hash::EntryHash::from_raw_32( bytes.to_vec() );
	let hhash = holo_hash::ActionHash::from_raw_32( bytes.to_vec() );

	let item = Entity {
	    id: ahash.clone(),
	    action: hhash,
	    address: ehash,
	    ctype: String::from("boolean"),
	    content: true,
	};

	assert_eq!( item.ctype, String::from("boolean") );
    }
}
