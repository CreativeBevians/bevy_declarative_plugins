use std::any::{TypeId, type_name};

use bevy_ecs::{resource::Resource, world::World};

use crate::{approval::Approval, erased::schedule::Staged, metadata_ptr::MetadataPtr};

/// A type erased [`Resource`], implemented using a [`MetadataPtr`]. This is
/// necessary due to [`Resource`] not being dyn compatible.
pub(crate) struct ErasedResource(pub(crate) MetadataPtr);

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct ResourceTypeId(pub(crate) TypeId);

impl ResourceTypeId {
    pub(crate) fn of<R: Resource>() -> Self {
        Self(TypeId::of::<R>())
    }
}

/// Data structure to get around the fact that Resource is not dyn compatible.
pub(crate) struct StagedResource {
    staged: Staged<Option<ErasedResource>>,
    type_id: ResourceTypeId,
    /// Function that un-erases the resource and check if it meets the plugin's requirements.
    pub(crate) approval_from_plugin: Approval<ErasedResource>,
    /// If the staged resource is a default, we care about it less than any given
    pub(crate) is_default: bool,
    pub(crate) resource_name: &'static str,
}

impl StagedResource {
    pub(crate) fn new<R: Resource>(
        resource: Option<R>,
        is_default: bool,
        approval: Approval<ErasedResource>,
    ) -> Option<Self> {
        Some(StagedResource {
            approval_from_plugin: approval,
            is_default,
            resource_name: type_name::<R>(),
            staged: Staged {
                erased: resource
                    .and_then(|resource| Some(ErasedResource(MetadataPtr::new(resource)?))),
                unerase_and_apply_to_world: |world, erased| {
                    if let Some(erased) = erased {
                        match erased.0.try_reverse_erase::<R>() {
                            Ok(resource) => world.insert_resource(resource),
                            Err(_) => {}
                        }
                    }
                },
            },
            type_id: ResourceTypeId::of::<R>(),
        })
    }

    pub(crate) fn apply_to_world(self, world: &mut World) {
        self.staged.apply_to_world(world);
    }

    pub(crate) fn erased(&self) -> Option<&ErasedResource> {
        self.staged.erased.as_ref()
    }

    #[allow(unused)]
    pub(crate) fn erased_type_id(&self) -> ResourceTypeId {
        self.type_id
    }
}
