//! Blocks.

mod argument;

pub use self::argument::Argument;
use super::{Location, Operation, OperationRef, RegionRef, Type, TypeLike, Value};
use crate::{
    context::Context,
    utility::{into_raw_array, print_callback},
    Error,
};
use mlir_sys::{
    mlirBlockAddArgument, mlirBlockAppendOwnedOperation, mlirBlockCreate, mlirBlockDestroy,
    mlirBlockDetach, mlirBlockEqual, mlirBlockGetArgument, mlirBlockGetFirstOperation,
    mlirBlockGetNextInRegion, mlirBlockGetNumArguments, mlirBlockGetParentOperation,
    mlirBlockGetParentRegion, mlirBlockGetTerminator, mlirBlockInsertOwnedOperation,
    mlirBlockInsertOwnedOperationAfter, mlirBlockInsertOwnedOperationBefore, mlirBlockPrint,
    MlirBlock,
};
use std::{
    ffi::c_void,
    fmt::{self, Debug, Display, Formatter},
    marker::PhantomData,
    mem::forget,
    ops::Deref,
};

/// A block
#[derive(Debug)]
pub struct Block<'c> {
    r#ref: BlockRef<'static>,
    _context: PhantomData<&'c Context>,
}

impl<'c> Block<'c> {
    /// Creates a block.
    pub fn new(arguments: &[(Type<'c>, Location<'c>)]) -> Self {
        unsafe {
            Self::from_raw(mlirBlockCreate(
                arguments.len() as isize,
                into_raw_array(
                    arguments
                        .iter()
                        .map(|(argument, _)| argument.to_raw())
                        .collect(),
                ),
                into_raw_array(
                    arguments
                        .iter()
                        .map(|(_, location)| location.to_raw())
                        .collect(),
                ),
            ))
        }
    }

    pub(crate) unsafe fn from_raw(raw: MlirBlock) -> Self {
        Self {
            r#ref: BlockRef::from_raw(raw),
            _context: Default::default(),
        }
    }

    pub(crate) unsafe fn into_raw(self) -> MlirBlock {
        let block = self.raw;

        forget(self);

        block
    }
}

impl<'c> Drop for Block<'c> {
    fn drop(&mut self) {
        unsafe { mlirBlockDestroy(self.raw) };
    }
}

impl<'c> PartialEq for Block<'c> {
    fn eq(&self, other: &Self) -> bool {
        self.r#ref == other.r#ref
    }
}

impl<'c> Eq for Block<'c> {}

impl<'c> Deref for Block<'c> {
    type Target = BlockRef<'static>;

    fn deref(&self) -> &Self::Target {
        &self.r#ref
    }
}

/// A reference of a block.
#[derive(Clone, Copy)]
pub struct BlockRef<'a> {
    raw: MlirBlock,
    _reference: PhantomData<&'a Block<'a>>,
}

impl<'c> BlockRef<'c> {
    /// Gets an argument at a position.
    pub fn argument(&self, position: usize) -> Result<Argument, Error> {
        unsafe {
            if position < self.argument_count() as usize {
                Ok(Argument::from_raw(mlirBlockGetArgument(
                    self.raw,
                    position as isize,
                )))
            } else {
                Err(Error::BlockArgumentPosition(self.to_string(), position))
            }
        }
    }

    /// Gets a number of arguments.
    pub fn argument_count(&self) -> usize {
        unsafe { mlirBlockGetNumArguments(self.raw) as usize }
    }

    /// Gets the first operation.
    pub fn first_operation(&self) -> Option<OperationRef> {
        unsafe {
            let operation = mlirBlockGetFirstOperation(self.raw);

            if operation.ptr.is_null() {
                None
            } else {
                Some(OperationRef::from_raw(operation))
            }
        }
    }

    /// Gets a terminator operation.
    pub fn terminator(&self) -> Option<OperationRef> {
        unsafe { OperationRef::from_option_raw(mlirBlockGetTerminator(self.raw)) }
    }

    /// Gets a parent region.
    pub fn parent_region(&self) -> Option<RegionRef> {
        unsafe { RegionRef::from_option_raw(mlirBlockGetParentRegion(self.raw)) }
    }

    /// Gets a parent operation.
    pub fn parent_operation(&self) -> Option<OperationRef> {
        unsafe { OperationRef::from_option_raw(mlirBlockGetParentOperation(self.raw)) }
    }

    /// Adds an argument.
    pub fn add_argument(&self, r#type: Type<'c>, location: Location<'c>) -> Value {
        unsafe {
            Value::from_raw(mlirBlockAddArgument(
                self.raw,
                r#type.to_raw(),
                location.to_raw(),
            ))
        }
    }

    /// Appends an operation.
    pub fn append_operation(&self, operation: Operation) -> OperationRef<'c> {
        unsafe {
            let operation = operation.into_raw();

            mlirBlockAppendOwnedOperation(self.raw, operation);

            OperationRef::from_raw(operation)
        }
    }

    /// Inserts an operation.
    // TODO How can we make those update functions take `&mut self`?
    // TODO Use cells?
    pub fn insert_operation(&self, position: usize, operation: Operation) -> OperationRef {
        unsafe {
            let operation = operation.into_raw();

            mlirBlockInsertOwnedOperation(self.raw, position as isize, operation);

            OperationRef::from_raw(operation)
        }
    }

    /// Inserts an operation after another.
    pub fn insert_operation_after(&self, one: OperationRef, other: Operation) -> OperationRef {
        unsafe {
            let other = other.into_raw();

            mlirBlockInsertOwnedOperationAfter(self.raw, one.to_raw(), other);

            OperationRef::from_raw(other)
        }
    }

    /// Inserts an operation before another.
    pub fn insert_operation_before(&self, one: OperationRef, other: Operation) -> OperationRef {
        unsafe {
            let other = other.into_raw();

            mlirBlockInsertOwnedOperationBefore(self.raw, one.to_raw(), other);

            OperationRef::from_raw(other)
        }
    }

    /// Detaches a block from a region and assumes its ownership.
    ///
    /// # Safety
    ///
    /// This function might invalidate existing references to the block if you
    /// drop it too early.
    // TODO Implement this for BlockRefMut instead and mark it safe.
    pub unsafe fn detach(&self) -> Option<Block> {
        if self.parent_region().is_some() {
            mlirBlockDetach(self.raw);

            Some(Block::from_raw(self.raw))
        } else {
            None
        }
    }

    /// Gets a next block in a region.
    pub fn next_in_region(&self) -> Option<BlockRef> {
        unsafe { BlockRef::from_option_raw(mlirBlockGetNextInRegion(self.raw)) }
    }

    pub(crate) unsafe fn from_raw(raw: MlirBlock) -> Self {
        Self {
            raw,
            _reference: Default::default(),
        }
    }

    pub(crate) unsafe fn from_option_raw(raw: MlirBlock) -> Option<Self> {
        if raw.ptr.is_null() {
            None
        } else {
            Some(Self::from_raw(raw))
        }
    }

    pub(crate) unsafe fn to_raw(self) -> MlirBlock {
        self.raw
    }
}

impl<'a> PartialEq for BlockRef<'a> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { mlirBlockEqual(self.raw, other.raw) }
    }
}

impl<'a> Eq for BlockRef<'a> {}

impl<'a> Display for BlockRef<'a> {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        let mut data = (formatter, Ok(()));

        unsafe {
            mlirBlockPrint(
                self.raw,
                Some(print_callback),
                &mut data as *mut _ as *mut c_void,
            );
        }

        data.1
    }
}

impl<'a> Debug for BlockRef<'a> {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        writeln!(formatter, "BlockRef(")?;
        Display::fmt(self, formatter)?;
        write!(formatter, ")")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        dialect,
        ir::{operation, Module, Region, ValueLike},
        utility::register_all_dialects,
    };

    #[test]
    fn new() {
        Block::new(&[]);
    }

    #[test]
    fn argument() {
        let context = Context::new();
        let r#type = Type::integer(&context, 64);

        assert_eq!(
            Block::new(&[(r#type, Location::unknown(&context))])
                .argument(0)
                .unwrap()
                .r#type(),
            r#type
        );
    }

    #[test]
    fn argument_error() {
        assert_eq!(
            Block::new(&[]).argument(0).unwrap_err(),
            Error::BlockArgumentPosition("<<UNLINKED BLOCK>>\n".into(), 0)
        );
    }

    #[test]
    fn argument_count() {
        assert_eq!(Block::new(&[]).argument_count(), 0);
    }

    #[test]
    fn parent_region() {
        let region = Region::new();
        let block = region.append_block(Block::new(&[]));

        assert_eq!(block.parent_region(), Some(*region));
    }

    #[test]
    fn parent_region_none() {
        let block = Block::new(&[]);

        assert_eq!(block.parent_region(), None);
    }

    #[test]
    fn parent_operation() {
        let context = Context::new();
        let module = Module::new(Location::unknown(&context));

        assert_eq!(
            module.body().parent_operation(),
            Some(module.as_operation())
        );
    }

    #[test]
    fn parent_operation_none() {
        let block = Block::new(&[]);

        assert_eq!(block.parent_operation(), None);
    }

    #[test]
    fn terminator() {
        let registry = dialect::Registry::new();
        register_all_dialects(&registry);

        let context = Context::new();
        context.append_dialect_registry(&registry);
        context.load_all_available_dialects();

        let block = Block::new(&[]);

        let operation = block.append_operation(
            operation::Builder::new("func.return", Location::unknown(&context)).build(),
        );

        assert_eq!(block.terminator(), Some(operation));
    }

    #[test]
    fn terminator_none() {
        assert_eq!(Block::new(&[]).terminator(), None);
    }

    #[test]
    fn first_operation() {
        let context = Context::new();
        let block = Block::new(&[]);

        let operation = block
            .append_operation(operation::Builder::new("foo", Location::unknown(&context)).build());

        assert_eq!(block.first_operation(), Some(operation));
    }

    #[test]
    fn first_operation_none() {
        let block = Block::new(&[]);

        assert_eq!(block.first_operation(), None);
    }

    #[test]
    fn append_operation() {
        let context = Context::new();
        let block = Block::new(&[]);

        block.append_operation(operation::Builder::new("foo", Location::unknown(&context)).build());
    }

    #[test]
    fn insert_operation() {
        let context = Context::new();
        let block = Block::new(&[]);

        block.insert_operation(
            0,
            operation::Builder::new("foo", Location::unknown(&context)).build(),
        );
    }

    #[test]
    fn insert_operation_after() {
        let context = Context::new();
        let block = Block::new(&[]);

        let first_operation = block
            .append_operation(operation::Builder::new("foo", Location::unknown(&context)).build());
        let second_operation = block.insert_operation_after(
            first_operation,
            operation::Builder::new("foo", Location::unknown(&context)).build(),
        );

        assert_eq!(block.first_operation(), Some(first_operation));
        assert_eq!(
            block.first_operation().unwrap().next_in_block(),
            Some(second_operation)
        );
    }

    #[test]
    fn insert_operation_before() {
        let context = Context::new();
        let block = Block::new(&[]);

        let second_operation = block
            .append_operation(operation::Builder::new("foo", Location::unknown(&context)).build());
        let first_operation = block.insert_operation_before(
            second_operation,
            operation::Builder::new("foo", Location::unknown(&context)).build(),
        );

        assert_eq!(block.first_operation(), Some(first_operation));
        assert_eq!(
            block.first_operation().unwrap().next_in_block(),
            Some(second_operation)
        );
    }

    #[test]
    fn next_in_region() {
        let region = Region::new();

        let first_block = region.append_block(Block::new(&[]));
        let second_block = region.append_block(Block::new(&[]));

        assert_eq!(first_block.next_in_region(), Some(second_block));
    }

    #[test]
    fn detach() {
        let region = Region::new();
        let block = region.append_block(Block::new(&[]));

        assert_eq!(
            unsafe { block.detach() }.unwrap().to_string(),
            "<<UNLINKED BLOCK>>\n"
        );
    }

    #[test]
    fn detach_detached() {
        let block = Block::new(&[]);

        assert!(unsafe { block.detach() }.is_none());
    }

    #[test]
    fn display() {
        assert_eq!(Block::new(&[]).to_string(), "<<UNLINKED BLOCK>>\n");
    }

    #[test]
    fn debug() {
        assert_eq!(
            format!("{:?}", *Block::new(&[])),
            "BlockRef(\n<<UNLINKED BLOCK>>\n)"
        );
    }
}
