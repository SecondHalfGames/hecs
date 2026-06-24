//! Extensions for hecs for MS80.

use alloc::boxed::Box;
use alloc::format;
use core::num::NonZeroU32;
use core::str::FromStr;
use std::fmt;
use std::sync::OnceLock;

use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::Entity;

static SERIALIZATION: OnceLock<Box<dyn EntitySerialization>> = OnceLock::new();

impl Entity {
    /// MS80 Extension: Generation of entity
    pub const fn generation(self) -> u32 {
        self.generation.get()
    }

    fn from_id_generation(id: u32, generation: u32) -> Option<Self> {
        Some(Self {
            id,
            generation: NonZeroU32::new(generation)?,
        })
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

pub enum SerializedEntity {
    /// Serialize this Entity as a string containing the exact ID and generation
    /// that's currently used in the world.
    Entity(Entity),

    /// Serialize this Entity as the given u64.
    Id(u64),
}

impl Serialize for SerializedEntity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match *self {
            SerializedEntity::Entity(entity) => {
                let label = format!("{}v{}", entity.id(), entity.generation());
                label.serialize(serializer)
            }
            SerializedEntity::Id(id) => serializer.serialize_u64(id),
        }
    }
}

/// MS80 Extension: Defines custom serialization for entities
#[allow(missing_docs)]
pub trait EntitySerialization: Send + Sync + 'static {
    fn entity_to_id(&self, entity: Entity) -> Option<SerializedEntity>;
    fn id_to_entity(&self, id: SerializedEntity) -> Option<Entity>;
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
            if serializer.is_human_readable() {
                match serialization.entity_to_id(*self) {
                    Some(s) => s.serialize(serializer),
                    None => serializer.serialize_none(),
                }
            } else {
                match serialization.entity_to_id(*self) {
                    Some(s) => s.serialize(serializer),
                    None => Entity::DANGLING.to_bits().serialize(serializer),
                }
            }
        } else {
            // No custom serialization was set; use default behavior.
            self.to_bits().serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for Entity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
        D::Error: serde::de::Error,
    {
        if SERIALIZATION.get().is_some() {
            if deserializer.is_human_readable() {
                // Human-readable formats can hold several representations of an
                // entity including a string, integer, or None.
                deserializer.deserialize_any(EntityHandleVisitor)
            } else {
                // Non-human-readable formats contain only serialized entity
                // IDs.
                deserializer.deserialize_u64(EntityHandleVisitor)
            }
        } else {
            // No custom deserialization set; use default behavior.
            let bits = u64::deserialize(deserializer)?;

            match Entity::from_bits(bits) {
                Some(ent) => Ok(ent),
                None => Err(serde::de::Error::invalid_value(
                    serde::de::Unexpected::Unsigned(bits),
                    &"`a valid `Entity` bitpattern",
                )),
            }
        }
    }
}

struct EntityHandleVisitor;

impl<'de> Visitor<'de> for EntityHandleVisitor {
    type Value = Entity;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "an integer entity ID")
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Entity::DANGLING)
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        v.parse().map_err(|_| E::custom("invalid entity"))
    }

    fn visit_u64<E>(self, id: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let mapped = SERIALIZATION
            .get()
            .and_then(|ser| ser.id_to_entity(SerializedEntity::Id(id)));

        let entity = match mapped {
            Some(entity) => entity,
            None => Entity::DANGLING,
        };

        Ok(entity)
    }
}
