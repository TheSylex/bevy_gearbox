use super::prelude::*;
use bevy::prelude::*;
use bevy_ecs::query::{QueryData, QueryFilter, WorldQuery};

pub trait GearboxQueryExt<'w, 's, D: QueryData, F: QueryFilter> {
    fn iter_child_sms(&'w self, entity: Entity) -> DescendantSMIter<'w, 's, D, F>
    where
        D::ReadOnly: WorldQuery<Item<'w> = &'w InChildSMState>;

    fn iter_parent_sms(&'w self, entity: Entity) -> AncestorSMIter<'w, 's, D, F>
    where
        D::ReadOnly: WorldQuery<Item<'w> = &'w Parent>;

    fn current_sm(&'w self, entity: Entity) -> Entity
    where
        D::ReadOnly: WorldQuery<Item<'w> = &'w InChildSMState>;

    fn ancestor_sm(&'w self, entity: Entity) -> Option<Entity>
    where
        D::ReadOnly: WorldQuery<Item<'w> = &'w Parent>;

    fn child_sm(&'w self, entity: Entity) -> Option<Entity>
    where
        D::ReadOnly: WorldQuery<Item<'w> = &'w InChildSMState>;

    fn parent_sm(&'w self, entity: Entity) -> Option<Entity>
    where
        D::ReadOnly: WorldQuery<Item<'w> = &'w Parent>;
}

impl<'w, 's, D: QueryData, F: QueryFilter> GearboxQueryExt<'w, 's, D, F>
    for Query<'w, 's, D, F>
{
    fn iter_child_sms(&'w self, entity: Entity) -> DescendantSMIter<'w, 's, D, F>
    where
        D::ReadOnly: WorldQuery<Item<'w> = &'w InChildSMState>,
    {
        DescendantSMIter::new(self, entity)
    }

    fn iter_parent_sms(&'w self, entity: Entity) -> AncestorSMIter<'w, 's, D, F>
    where
        D::ReadOnly: WorldQuery<Item<'w> = &'w Parent>,
    {
        AncestorSMIter::new(self, entity)
    }

    fn current_sm(&'w self, entity: Entity) -> Entity
    where
        D::ReadOnly: WorldQuery<Item<'w> = &'w InChildSMState>,
    {
        self.iter_child_sms(entity).last().unwrap_or(entity)
    }

    fn ancestor_sm(&'w self, entity: Entity) -> Option<Entity>
    where
        D::ReadOnly: WorldQuery<Item<'w> = &'w Parent>,
    {
        self.iter_parent_sms(entity).last()
    }

    fn child_sm(&'w self, entity: Entity) -> Option<Entity>
    where
        D::ReadOnly: WorldQuery<Item<'w> = &'w InChildSMState>,
    {
        self.iter_child_sms(entity).next()
    }

    fn parent_sm(&'w self, entity: Entity) -> Option<Entity>
    where
        D::ReadOnly: WorldQuery<Item<'w> = &'w Parent>,
    {
        self.iter_parent_sms(entity).next()
    }
}





/// An [`Iterator`] of [`Entity`]s over the descendants of an [`Entity`].
///
/// Traverses the hierarchy breadth-first.
pub struct DescendantSMIter<'w, 's, D: QueryData, F: QueryFilter>
where
    D::ReadOnly: WorldQuery<Item<'w> = &'w InChildSMState>,
{
    in_child_sm_query: &'w Query<'w, 's, D, F>,
    next: Option<Entity>,
}

impl<'w, 's, D: QueryData, F: QueryFilter> DescendantSMIter<'w, 's, D, F>
where
    D::ReadOnly: WorldQuery<Item<'w> = &'w InChildSMState>,
{
    /// Returns a new [`AncestorIter`].
    pub fn new(parent_query: &'w Query<'w, 's, D, F>, entity: Entity) -> Self {
        DescendantSMIter {
            in_child_sm_query: parent_query,
            next: Some(entity),
        }
    }
}

impl<'w, 's, D: QueryData, F: QueryFilter> Iterator for DescendantSMIter<'w, 's, D, F>
where
    D::ReadOnly: WorldQuery<Item<'w> = &'w InChildSMState>,
{
    type Item = Entity;

    fn next(&mut self) -> Option<Self::Item> {
        self.next = self.in_child_sm_query.get(self.next?).ok().map(|p| p.0);
        self.next
    }
}





/// An [`Iterator`] of [`Entity`]s over the ancestors of an [`Entity`].
pub struct AncestorSMIter<'w, 's, D: QueryData, F: QueryFilter>
where
    D::ReadOnly: WorldQuery<Item<'w> = &'w Parent>,
{
    parent_sm_query: &'w Query<'w, 's, D, F>,
    next: Option<Entity>,
}

impl<'w, 's, D: QueryData, F: QueryFilter> AncestorSMIter<'w, 's, D, F>
where
    D::ReadOnly: WorldQuery<Item<'w> = &'w Parent>,
{
    /// Returns a new [`AncestorIter`].
    pub fn new(parent_query: &'w Query<'w, 's, D, F>, entity: Entity) -> Self {
        AncestorSMIter {
            parent_sm_query: parent_query,
            next: Some(entity),
        }
    }
}

impl<'w, 's, D: QueryData, F: QueryFilter> Iterator for AncestorSMIter<'w, 's, D, F>
where
    D::ReadOnly: WorldQuery<Item<'w> = &'w Parent>,
{
    type Item = Entity;

    fn next(&mut self) -> Option<Self::Item> {
        self.next = self.parent_sm_query.get(self.next?).ok().map(|p| p.get());
        self.next
    }
}