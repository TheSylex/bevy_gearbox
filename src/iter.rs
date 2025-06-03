use super::prelude::*;
use bevy_ecs::{
    entity::Entity,
    query::{QueryData, QueryFilter, WorldQuery},
    system::Query,
};

pub trait HierarchyQueryExt<'w, 's, D: QueryData, F: QueryFilter> {
    fn iter_child_sms(&'w self, entity: Entity) -> DescendantIter<'w, 's, D, F>
    where
        D::ReadOnly: WorldQuery<Item<'w> = &'w InChildSMState>;

    fn iter_parent_sms(&'w self, entity: Entity) -> AncestorIter<'w, 's, D, F>
    where
        D::ReadOnly: WorldQuery<Item<'w> = &'w Parent>;

    fn current_sm(&'w self, entity: Entity) -> Option<Entity>
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

impl<'w, 's, D: QueryData, F: QueryFilter> HierarchyQueryExt<'w, 's, D, F>
    for Query<'w, 's, D, F>
{
    fn iter_child_sms(&'w self, entity: Entity) -> DescendantIter<'w, 's, D, F>
    where
        D::ReadOnly: WorldQuery<Item<'w> = &'w InChildSMState>,
    {
        DescendantIter::new(self, entity)
    }

    fn iter_parent_sms(&'w self, entity: Entity) -> AncestorIter<'w, 's, D, F>
    where
        D::ReadOnly: WorldQuery<Item<'w> = &'w Parent>,
    {
        AncestorIter::new(self, entity)
    }

    fn current_sm(&'w self, entity: Entity) -> Option<Entity>
    where
        D::ReadOnly: WorldQuery<Item<'w> = &'w InChildSMState>,
    {
        self.iter_child_sms(entity).last()
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
pub struct DescendantIter<'w, 's, D: QueryData, F: QueryFilter>
where
    D::ReadOnly: WorldQuery<Item<'w> = &'w InChildSMState>,
{
    in_child_sm_query: &'w Query<'w, 's, D, F>,
    next: Option<Entity>,
}

impl<'w, 's, D: QueryData, F: QueryFilter> DescendantIter<'w, 's, D, F>
where
    D::ReadOnly: WorldQuery<Item<'w> = &'w InChildSMState>,
{
    /// Returns a new [`AncestorIter`].
    pub fn new(parent_query: &'w Query<'w, 's, D, F>, entity: Entity) -> Self {
        DescendantIter {
            in_child_sm_query: parent_query,
            next: Some(entity),
        }
    }
}

impl<'w, 's, D: QueryData, F: QueryFilter> Iterator for DescendantIter<'w, 's, D, F>
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
pub struct AncestorIter<'w, 's, D: QueryData, F: QueryFilter>
where
    D::ReadOnly: WorldQuery<Item<'w> = &'w Parent>,
{
    parent_sm_query: &'w Query<'w, 's, D, F>,
    next: Option<Entity>,
}

impl<'w, 's, D: QueryData, F: QueryFilter> AncestorIter<'w, 's, D, F>
where
    D::ReadOnly: WorldQuery<Item<'w> = &'w Parent>,
{
    /// Returns a new [`AncestorIter`].
    pub fn new(parent_query: &'w Query<'w, 's, D, F>, entity: Entity) -> Self {
        AncestorIter {
            parent_sm_query: parent_query,
            next: Some(entity),
        }
    }
}

impl<'w, 's, D: QueryData, F: QueryFilter> Iterator for AncestorIter<'w, 's, D, F>
where
    D::ReadOnly: WorldQuery<Item<'w> = &'w Parent>,
{
    type Item = Entity;

    fn next(&mut self) -> Option<Self::Item> {
        self.next = self.parent_sm_query.get(self.next?).ok().map(|p| p.get());
        self.next
    }
}