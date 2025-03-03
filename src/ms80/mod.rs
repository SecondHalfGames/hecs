//! Extensions for hecs for MS80.

use alloc::boxed::Box;
use alloc::format;
use core::str::FromStr;
use serde::de::Visitor;
use std::fmt::{self};
use std::string::String;
use std::sync::OnceLock;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::Entity;

static SERIALIZATION: OnceLock<Box<dyn EntitySerialization>> = OnceLock::new();

impl Entity {
    /// MS80 Extension: Generation of entity
    pub const fn generation(self) -> u32 {
        self.generation.get()
    }

    fn from_id_generation(id: u32, generation: u32) -> Option<Self> {
        let generation = (generation as u64) << 32;
        let id = id as u64;
        Self::from_bits(generation | id)
    }
}

impl FromStr for Entity {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut split = s.splitn(2, 'v');
        let id = split.next().unwrap().parse().map_err(drop)?;
        let generation = split.next().ok_or(())?.parse().map_err(drop)?;

        Self::from_id_generation(id, generation).ok_or(())
    }
}

/// MS80 Extension: Defines custom serialization for entities
#[allow(missing_docs)]
pub trait EntitySerialization: Send + Sync + 'static {
    fn entity_to_id(&self, entity: Entity) -> Option<u64>;
    fn id_to_entity(&self, id: u64) -> Option<Entity>;
    fn is_deserializing(&self) -> bool;
}

/// MS80 Extension: Set the current entity serializer; can only be called once.
pub fn set_entity_serialization<T: EntitySerialization>(value: T) -> bool {
    SERIALIZATION.set(Box::new(value)).is_ok()
}

impl Serialize for Entity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if let Some(serialization) = SERIALIZATION.get() {
            if let Some(id) = serialization.entity_to_id(*self) {
                return serializer.serialize_u64(id);
            }
        }

        let label = format!("{}v{}", self.id(), self.generation());
        label.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Entity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
        D::Error: serde::de::Error,
    {
        if let Some(serialization) = SERIALIZATION.get() {
            if serialization.is_deserializing() {
                return deserializer.deserialize_u64(EntityHandleVisitor);
            }
        }

        let label = String::deserialize(deserializer)?;
        let handle: Entity = label
            .parse()
            .map_err(|_| D::Error::custom("invalid entity"))?;

        Ok(handle)
    }
}

struct EntityHandleVisitor;

impl<'de> Visitor<'de> for EntityHandleVisitor {
    type Value = Entity;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "an integer entity ID")
    }

    fn visit_u64<E>(self, id: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let mapped = SERIALIZATION.get().and_then(|ser| ser.id_to_entity(id));

        let entity = match mapped {
            Some(entity) => entity,
            None => Entity::from_bits(id).ok_or_else(|| E::custom("invalid hecs entity ID"))?,
        };

        Ok(entity)
    }
}
