use super::prelude::*;
use bevy::prelude::*;
use bevy_ecs::query::{QueryData, QueryFilter};

pub trait GearboxQueryExt<'w, 's, D: QueryData, F: QueryFilter> {
    fn iter_child_sms(&'w self, entity: Entity) -> DescendantSMIter<'w, 's, D, F>
    where
        D::ReadOnly: QueryData<Item<'w> = &'w InChildSMState>;

    fn iter_child_of_sms(&'w self, entity: Entity) -> AncestorSMIter<'w, 's, D, F>
    where
        D::ReadOnly: QueryData<Item<'w> = &'w ChildOf>;

    fn current_sm(&'w self, entity: Entity) -> Entity
    where
        D::ReadOnly: QueryData<Item<'w> = &'w InChildSMState>;

    fn ancestor_sm(&'w self, entity: Entity) -> Entity
    where
        D::ReadOnly: QueryData<Item<'w> = &'w ChildOf>;

    fn child_sm(&'w self, entity: Entity) -> Option<Entity>
    where
        D::ReadOnly: QueryData<Item<'w> = &'w InChildSMState>;

    fn parent_sm(&'w self, entity: Entity) -> Option<Entity>
    where
        D::ReadOnly: QueryData<Item<'w> = &'w ChildOf>;
}

impl<'w, 's, D: QueryData, F: QueryFilter> GearboxQueryExt<'w, 's, D, F>
    for Query<'w, 's, D, F>
{
    fn iter_child_sms(&'w self, entity: Entity) -> DescendantSMIter<'w, 's, D, F>
    where
        D::ReadOnly: QueryData<Item<'w> = &'w InChildSMState>,
    {
        DescendantSMIter::new(self, entity)
    }

    fn iter_child_of_sms(&'w self, entity: Entity) -> AncestorSMIter<'w, 's, D, F>
    where
        D::ReadOnly: QueryData<Item<'w> = &'w ChildOf>,
    {
        AncestorSMIter::new(self, entity)
    }

    fn current_sm(&'w self, entity: Entity) -> Entity
    where
        D::ReadOnly: QueryData<Item<'w> = &'w InChildSMState>,
    {
        self.iter_child_sms(entity).last().unwrap_or(entity)
    }

    fn ancestor_sm(&'w self, entity: Entity) -> Entity
    where
        D::ReadOnly: QueryData<Item<'w> = &'w ChildOf>,
    {
        self.iter_child_of_sms(entity).last().unwrap_or(entity)
    }

    fn child_sm(&'w self, entity: Entity) -> Option<Entity>
    where
        D::ReadOnly: QueryData<Item<'w> = &'w InChildSMState>,
    {
        self.iter_child_sms(entity).next()
    }

    fn parent_sm(&'w self, entity: Entity) -> Option<Entity>
    where
        D::ReadOnly: QueryData<Item<'w> = &'w ChildOf>,
    {
        self.iter_child_of_sms(entity).next()
    }
}





/// An [`Iterator`] of [`Entity`]s over the descendants of an [`Entity`].
///
/// Traverses the hierarchy breadth-first.
pub struct DescendantSMIter<'w, 's, D: QueryData, F: QueryFilter>
where
    D::ReadOnly: QueryData<Item<'w> = &'w InChildSMState>,
{
    in_child_sm_query: &'w Query<'w, 's, D, F>,
    next: Option<Entity>,
}

impl<'w, 's, D: QueryData, F: QueryFilter> DescendantSMIter<'w, 's, D, F>
where
    D::ReadOnly: QueryData<Item<'w> = &'w InChildSMState>,
{
    /// Returns a new [`AncestorIter`].
    pub fn new(in_child_sm_query: &'w Query<'w, 's, D, F>, entity: Entity) -> Self {
        DescendantSMIter {
            in_child_sm_query,
            next: Some(entity),
        }
    }
}

impl<'w, 's, D: QueryData, F: QueryFilter> Iterator for DescendantSMIter<'w, 's, D, F>
where
    D::ReadOnly: QueryData<Item<'w> = &'w InChildSMState>,
{
    type Item = Entity;

    fn next(&mut self) -> Option<Self::Item> {
        self.next = self.in_child_sm_query.get(self.next?).ok().map(|c| c.0);
        self.next
    }
}





/// An [`Iterator`] of [`Entity`]s over the ancestors of an [`Entity`].
pub struct AncestorSMIter<'w, 's, D: QueryData, F: QueryFilter>
where
    D::ReadOnly: QueryData<Item<'w> = &'w ChildOf>,
{
    child_of_sm_query: &'w Query<'w, 's, D, F>,
    next: Option<Entity>,
}

impl<'w, 's, D: QueryData, F: QueryFilter> AncestorSMIter<'w, 's, D, F>
where
    D::ReadOnly: QueryData<Item<'w> = &'w ChildOf>,
{
    /// Returns a new [`AncestorIter`].
    pub fn new(child_of_sm_query: &'w Query<'w, 's, D, F>, entity: Entity) -> Self {
        AncestorSMIter {
            child_of_sm_query: child_of_sm_query,
            next: Some(entity),
        }
    }
}

impl<'w, 's, D: QueryData, F: QueryFilter> Iterator for AncestorSMIter<'w, 's, D, F>
where
    D::ReadOnly: QueryData<Item<'w> = &'w ChildOf>,
{
    type Item = Entity;

    fn next(&mut self) -> Option<Self::Item> {
        self.next = self.child_of_sm_query.get(self.next?).ok().map(|p| p.parent());
        self.next
    }
}