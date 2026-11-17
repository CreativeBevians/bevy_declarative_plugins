use std::any::{TypeId, type_name};

use bevy_ecs::{resource::Resource, world::World};

use crate::{
    PluginOutput,
    approval::{Approval, WithApproval},
    erased::schedule::Staged,
    metadata_ptr::MetadataPtr,
};

/// A type erased [`Resource`], implemented using a [`MetadataPtr`].
pub(crate) struct ErasedResource(pub(crate) MetadataPtr);

/// [`TypeId`] associated with a [`Resource`].
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct ResourceTypeId(pub(crate) TypeId);

impl ResourceTypeId {
    /// [`TypeId::of`] but for [`Resource`] types.
    pub(crate) fn of<R: Resource>() -> Self {
        Self(TypeId::of::<R>())
    }
}

/// A [`Staged`] resource and additional metadata for use during solving which
/// instance of a resource to pick.
pub(crate) struct StagedResource {
    /// Staged resource, an `Option` because a registered resource may not have
    /// a value associated with it when a plugin assumes another .
    staged: Staged<Option<ErasedResource>>,
    type_id: ResourceTypeId,
    /// [`Approval`] function for if the resource meets this plugin's needs.
    pub(crate) approval_from_plugin: Approval<ErasedResource>,
    /// If the staged resource is a default, we care about it less than a given
    /// non-default instance of that resource.
    pub(crate) is_default: bool,
    /// The type name of the resource, used for error reporting.
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
                erased: resource.and_then(|resource| {
                    Some(ErasedResource(
                        MetadataPtr::new(resource).expect("Allocation failure."),
                    ))
                }),
                unerase_and_apply_to_world: Self::reverse_erase_apply_to_world::<R>,
            },
            type_id: ResourceTypeId::of::<R>(),
        })
    }

    fn reverse_erase_apply_to_world<R: Resource>(
        world: &mut World,
        erased: Option<ErasedResource>,
    ) {
        if let Some(erased) = erased {
            match erased.0.try_reverse_erase::<R>() {
                Ok(resource) => world.insert_resource(resource),
                Err(_) => {}
            }
        }
    }

    /// Pass-through method for [`Staged::apply_to_world`].
    pub(crate) fn apply_to_world(self, world: &mut World) {
        self.staged.apply_to_world(world);
    }

    /// Get the erased resource as a reference.
    pub(crate) fn get_erased(&self) -> Option<&ErasedResource> {
        self.staged.erased.as_ref()
    }

    /// Get the erased resource's [`ResourceTypeId`].
    #[allow(unused)]
    pub(crate) fn erased_type_id(&self) -> ResourceTypeId {
        self.type_id
    }
}

/// Helper trait for making adding resources to a plugin easier. Nearly
/// identical to [`crate::entry_points::AddDependency`].
pub trait AddResource: Sized {
    type Resource: Resource;

    /// Add `self` to the [`PluginOutput`] of a plugin.
    fn add_to_output<D>(self, output: &mut PluginOutput<D>);

    /// Create a version of this resource with an approval function.
    fn with_approval(
        self,
        f: impl for<'a> Fn(&'a Self::Resource) -> bool + 'static,
    ) -> WithApproval<Self, Self::Resource>
    where
        Self: 'static,
    {
        WithApproval {
            subject: self,
            approval: Box::new(f),
        }
    }

    /// Get the underlying resource value.
    fn get_value(self) -> Option<Self::Resource>;
}

impl<R> AddResource for R
where
    R: Resource,
{
    type Resource = R;

    fn add_to_output<D>(self, output: &mut PluginOutput<D>) {
        output.resource_builder().with_data(self).build();
    }

    fn get_value(self) -> Option<Self::Resource> {
        Some(self)
    }
}

impl<S> AddResource for WithApproval<S, S::Resource>
where
    S: AddResource,
{
    type Resource = S::Resource;

    fn add_to_output<D>(self, output: &mut PluginOutput<D>) {
        let WithApproval { subject, approval } = self;
        match subject.get_value() {
            Some(value) => output
                .resource_builder()
                .with_data(value)
                .with_approval(approval)
                .build(),
            None => todo!(),
        };
    }

    fn get_value(self) -> Option<Self::Resource> {
        todo!()
    }

    /// Calling `with_approval` on a [`WithApproval`] type will AND the two
    /// approval functions, "nested" [`WithApproval`] instances will have their
    /// functions replaced with panicking approval functions as only the
    /// topmost approval should be used.
    fn with_approval(
        self,
        f: impl for<'a> Fn(&'a Self::Resource) -> bool + 'static,
    ) -> WithApproval<Self, Self::Resource>
    where
        Self: 'static,
    {
        let WithApproval {
            subject,
            approval: prev_approval,
        } = self;

        WithApproval {
            subject: WithApproval {
                subject,
                // We avoid using the inner one, because we instead AND the
                // previous approval with the new approval.
                approval: Box::new(|_value: &S::Resource| {
                    panic!("Should not use inner approval function")
                }),
            },
            approval: Box::new(move |value: &S::Resource| f(value) && prev_approval(value)),
        }
    }
}
