//! SIZE selection follows IM Toolbox Essentials, "The Size Resource".
use super::{invalid, resource::ResourceFork, u16_at, u32_at, IMAGE_LIMIT, STACK_SIZE};
use crate::Result;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplicationSize {
    pub resource_id: i16,
    pub flags: u16,
    pub preferred: u32,
    pub minimum: u32,
}
impl ApplicationSize {
    pub(super) fn read(resources: &ResourceFork) -> Result<Option<Self>> {
        let Some(resource) = resources
            .get(*b"SIZE", 0)
            .or_else(|| resources.get(*b"SIZE", -1))
        else {
            return Ok(None);
        };
        if resource.attributes & 1 != 0 {
            return Err(invalid("compressed SIZE resources unsupported"));
        }
        if resource.data.len() != 10 {
            return Err(invalid("SIZE resource must contain flags and two sizes"));
        }
        let request = Self {
            resource_id: resource.id,
            flags: u16_at(&resource.data, 0)?,
            preferred: u32_at(&resource.data, 2)?,
            minimum: u32_at(&resource.data, 6)?,
        };
        if request.minimum == 0
            || request.preferred < request.minimum
            || request.preferred as usize > IMAGE_LIMIT
            || request.preferred <= STACK_SIZE + 256
        {
            return Err(invalid(
                "unsupported or invalid SIZE partition requirements",
            ));
        }
        Ok(Some(request))
    }
    pub(super) fn aligned_size(self) -> Result<u32> {
        self.preferred
            .checked_add(15)
            .map(|n| n & !15)
            .ok_or_else(|| invalid("SIZE partition alignment overflow"))
    }
}
