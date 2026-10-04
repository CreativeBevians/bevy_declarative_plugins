use std::{
    any::{TypeId, type_name},
    default,
};

use bevy_ecs::{resource::Resource, world::World};

use crate::{approval::Approval, erased::schedule::Staged, metadata_ptr::MetadataPtr};

/// A type erased [`Resource`], implemented using a [`MetadataPtr`]. This is
/// necessary due to [`Resource`] not being dyn compatible.
pub(crate) struct ErasedResource(pub(crate) MetadataPtr);

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub(crate) struct ResourceTypeId(pub(crate) TypeId);

impl ResourceTypeId {
    pub(crate) fn of<R: Resource>() -> Self {
        Self(TypeId::of::<R>())
    }
}

/// Data structure to get around the fact that Resource is not dyn compatible.
pub(crate) struct StagedResource {
    staged: Staged<ErasedResource>,
    /// Function that un-erases the resource and check if it meets the plugin's requirements.
    pub(crate) approval_from_plugin: Approval<ErasedResource>,
    /// If the staged resource is a default, we care about it less than any given
    pub(crate) is_default: bool,
    pub(crate) resource_name: &'static str,
}

impl StagedResource {
    pub(crate) fn new<R: Resource>(
        resource: R,
        is_default: bool,
        check_ok: impl Fn(&R) -> bool + 'static,
    ) -> Option<Self> {
        Some(StagedResource {
            approval_from_plugin: Approval::new(move |erased: &ErasedResource| {
                erased
                    .0
                    .visit::<R, _>(|data| check_ok(data))
                    // If the types didn't match, we should never veto (it's not this type's problem).
                    .unwrap_or(true)
            }),
            is_default,
            resource_name: type_name::<R>(),
            staged: Staged {
                erased: ErasedResource(MetadataPtr::new(resource)?),
                unerase_and_apply_to_world: |world, erased| match erased.0.try_reverse_erase::<R>()
                {
                    Ok(resource) => world.insert_resource(resource),
                    Err(_) => {}
                },
            },
        })
    }

    fn new_with_default<R: Resource + Default>(
        check_ok: impl Fn(&R) -> bool + 'static,
    ) -> Option<Self> {
        Self::new(R::default(), true, check_ok)
    }

    pub(crate) fn apply_to_world(self, world: &mut World) {
        self.staged.apply_to_world(world);
    }

    pub(crate) fn erased(&self) -> &ErasedResource {
        &self.staged.erased
    }

    pub(crate) fn erased_type_id(&self) -> ResourceTypeId {
        // SAFETY: We know this is a Resource as the constructor requires R to
        // be a resource.
        ResourceTypeId(self.staged.erased.0.erased_type_id())
    }
}
