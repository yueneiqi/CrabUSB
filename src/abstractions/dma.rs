use core::{
    alloc::{Allocator, Layout},
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::{slice_from_raw_parts, slice_from_raw_parts_mut, NonNull},
};

use super::PlatformAbstractions;

pub struct DMA<T, O>
where
    T: ?Sized,
    O: PlatformAbstractions,
{
    layout: Layout,
    data: NonNull<[u8]>,
    allocator: O::DMA,
    __marker: PhantomData<T>,
}

unsafe impl<T, O> Sync for DMA<T, O>
where
    T: ?Sized,
    O: PlatformAbstractions,
{
}

unsafe impl<T, O> Send for DMA<T, O>
where
    T: ?Sized,
    O: PlatformAbstractions,
{
}

impl<T, O> DMA<T, O>
where
    T: Sized,
    O: PlatformAbstractions,
{
    /// 从 `value` `align` 和 `allocator` 创建 DMA，
    /// 若不符合以下条件则 Panic `LayoutError`：
    ///
    /// * `align` 不能为 0，
    ///
    /// * `align` 必须是2的幂次方。
    pub fn new(value: T, align: usize, allocator: O::DMA) -> Self {
        //计算所需内存大小
        let buff_size = size_of::<T>();
        // 根据元素数量和对其要求创建内存布局
        let layout = Layout::from_size_align(buff_size, align).unwrap();
        // 使用分配器分配内存
        let data = allocator.allocate(layout).unwrap();
        let ptr = data.cast();
        unsafe {
            ptr.write(value);
        };
        Self {
            layout,
            data,
            allocator,
            __marker: PhantomData,
        }
    }

    pub fn fill_zero(mut self) -> Self {
        unsafe { self.data.as_mut().iter_mut().for_each(|u| *u = 0u8) }
        self
    }
}

impl<T, O> DMA<T, O>
where
    T: ?Sized,
    O: PlatformAbstractions,
{
    /// 返回 [DMA] 地址
    pub fn addr(&self) -> O::VirtAddr {
        self.data.addr().get().into()
    }

    pub fn length_for_bytes(&self) -> usize {
        self.layout.size()
    }

    pub fn phys_addr_len_tuple(&self) -> AddrLenTuple<O> {
        AddrLenTuple(O::PhysAddr::from(self.addr()), self.length_for_bytes())
    }
}

pub struct AddrLenTuple<O: PlatformAbstractions>(O::PhysAddr, usize);

impl<O> From<AddrLenTuple<O>> for (usize, usize)
where
    O: PlatformAbstractions,
{
    fn from(value: AddrLenTuple<O>) -> Self {
        (value.0.into(), value.1)
    }
}

impl<T, O> DMA<[T], O>
where
    T: Sized,
    O: PlatformAbstractions,
{
    pub fn zeroed(count: usize, align: usize, allocator: O::DMA) -> Self {
        let t_size = size_of::<T>();
        let size = count * t_size;

        // 根据元素数量和对其要求创建内存布局
        let layout = Layout::from_size_align(size, align).unwrap();
        // 使用分配器分配内存
        let mut data = allocator.allocate(layout).unwrap();

        unsafe {
            for one in data.as_mut() {
                *one = 0;
            }
        }

        Self {
            layout,
            data,
            allocator,
            __marker: PhantomData,
        }
    }

    pub fn new_vec(init: T, count: usize, align: usize, allocator: O::DMA) -> Self {
        let t_size = size_of::<T>();
        let size = count * t_size;

        // 根据元素数量和对其要求创建内存布局
        let layout = Layout::from_size_align(size, align).unwrap();
        // 使用分配器分配内存
        let mut data = allocator.allocate(layout).unwrap();
        // debug!("allocated data:{:?}", data);

        unsafe {
            for i in 0..count {
                let data = &mut data.as_mut()[i * t_size..(i + 1) * t_size];
                let t_ptr = &init as *const T as *const u8;
                let t_data = &*slice_from_raw_parts(t_ptr, t_size);
                data.copy_from_slice(t_data);
            }
        }

        Self {
            layout,
            data,
            allocator,
            __marker: PhantomData,
        }
    }
}

impl<T, O> Deref for DMA<[T], O>
where
    O: PlatformAbstractions,
{
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        unsafe {
            let len = self.data.len() / size_of::<T>();
            let ptr = self.data.cast::<T>();
            &*slice_from_raw_parts(ptr.as_ptr(), len)
        }
    }
}

impl<T, O> DerefMut for DMA<[T], O>
where
    O: PlatformAbstractions,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {
            let len = self.data.len() / size_of::<T>();
            let ptr = self.data.cast::<T>().as_ptr();
            &mut *slice_from_raw_parts_mut(ptr, len)
        }
    }
}

impl<T, O> Deref for DMA<T, O>
where
    O: PlatformAbstractions,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe {
            let ptr = self.data.cast::<T>();
            ptr.as_ref()
        }
    }
}

impl<T, O> DerefMut for DMA<T, O>
where
    O: PlatformAbstractions,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {
            let mut ptr = self.data.cast::<T>();
            ptr.as_mut()
        }
    }
}
impl<T, O> Drop for DMA<T, O>
where
    T: ?Sized,
    O: PlatformAbstractions,
{
    fn drop(&mut self) {
        unsafe {
            let ptr = self.data.cast::<u8>();
            self.allocator.deallocate(ptr, self.layout);
        }
    }
}
