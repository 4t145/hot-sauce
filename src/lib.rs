use std::sync::{
    atomic::{AtomicPtr, AtomicUsize, Ordering},
    Arc, Weak,
};
#[derive(Debug)]
struct Version(AtomicUsize);

impl Version {
    #[inline]
    fn inc(&self) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    #[inline]
    fn get(&self) -> usize {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// A source to provides hot data
/// ```rust
/// # use hot_sauce::{HotSource, Hot};
/// let source = HotSource::<str>::new("hello world");
/// let mut hot_str = source.get();
/// source.update("hello hotsauce");
/// assert!(hot_str.is_expired());
/// hot_str.sync();
/// assert!(!hot_str.is_expired());
/// assert_eq!(&*hot_str, "hello hotsauce");
/// ```
#[derive(Debug, Clone)]
#[repr(transparent)]
struct HotSource<T: ?Sized>(Arc<HotSourceInner<T>>);

impl<T: ?Sized> HotSource<T> {
    pub fn new(data: impl Into<Arc<T>>) -> Self {
        HotSource(HotSourceInner::new(data))
    }
}

impl<T: ?Sized> std::ops::Deref for HotSource<T> {
    type Target = Arc<HotSourceInner<T>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
struct HotSourceInner<T: ?Sized> {
    /// version is used to check if the data is expired
    version: Version,
    /// data is the actual data
    data: AtomicPtr<Weak<T>>,
}

impl<T: ?Sized> HotSourceInner<T> {
    /// create a new hot source
    pub fn new(data: impl Into<Arc<T>>) -> Arc<Self> {
        let a_data: Arc<T> = data.into();
        // hold
        unsafe {
            Arc::increment_strong_count(a_data.as_ref() as *const T);
        }
        // let p_data = Arc::as_ptr(&a_data);
        // unsafe {
        //     Arc::increment_strong_count(p_data)
        // };
        let b_data = Box::new(Arc::downgrade(&a_data));
        let p = Box::leak(b_data) as *const Weak<T> as *mut Weak<T>;
        let ap_data = AtomicPtr::new(p);
        Arc::new(Self {
            version: Version(AtomicUsize::new(0)),
            data: ap_data,
        })
    }

    /// update value from source
    pub fn update(&self, new_data: impl Into<Arc<T>>) {
        let arc_data: Arc<T> = new_data.into();
        // hold
        unsafe {
            Arc::increment_strong_count(arc_data.as_ref() as *const T);
        }
        let b_data = Box::new(Arc::downgrade(&arc_data));
        let p = Box::leak(b_data) as *const Weak<T> as *mut Weak<T>;
        self.version.inc();
        let p_older = self.data.swap(p, Ordering::SeqCst);
        let _ = unsafe { Box::from_raw(p_older) };
        // release
        unsafe { Arc::decrement_strong_count(p_older.cast_const()) };
    }

    /// get a `Hot` pointer to the data
    pub fn get(self: &Arc<Self>) -> Hot<T> {
        // read version first
        let version = self.version.get();
        let p_data = self.data.load(Ordering::SeqCst);
        // we just de readonly operations
        let data = unsafe { p_data.as_ref().expect("invalid hot pointer") }.clone();
        if let Some(data) = data.upgrade() {
            Hot {
                version,
                data,
                source: self.clone(),
            }
        } else {
            panic!("invalid weak");
            self.get()
        }
    }
}

impl<T: ?Sized> Drop for HotSourceInner<T> {
    fn drop(&mut self) {
        let p_older = self.data.load(Ordering::SeqCst);
        // it's ok to do so as we guarentee this will drop only when all spawned Hot has been dropped,
        // at that time, no one can modify the data pointer
        let _ = unsafe { Box::from_raw(p_older) };
        if cfg!(test) {
            println!("drop at version {:?}", self.version)
        }
    }
}

/// A `Hot` pointer is used to wrap a dynamically updated data
#[derive(Debug)]
pub struct Hot<T: ?Sized> {
    version: usize,
    data: Arc<T>,
    source: Arc<HotSourceInner<T>>,
}

impl<T: ?Sized> Clone for Hot<T> {
    fn clone(&self) -> Self {
        Self {
            version: self.version,
            data: self.data.clone(),
            source: self.source.clone(),
        }
    }
}

impl<T: ?Sized> Hot<T> {
    pub fn new(data: impl Into<Arc<T>>) -> Self {
        HotSource::new(data).get()
    }
    /// update the pointee content
    pub fn update(&mut self, new_data: impl Into<Arc<T>>) {
        self.source.update(new_data.into());
        *self = self.source.get();
    }

    /// get the cached data (it may not be the newest value)
    pub fn get(&self) -> &T {
        &self.data
    }

    /// check if current data has the newest version
    pub fn is_expired(&self) -> bool {
        self.version < self.source.version.get()
    }

    /// sync the cached data to newest version
    pub fn sync(&mut self) -> &mut Self {
        *self = self.source.get();
        self
    }

    /// it's a combination of [#method.sync] and
    pub fn get_sync(&mut self) -> &T {
        if self.is_expired() {
            self.sync().get_sync()
        } else {
            self.get()
        }
    }
}

impl<T: ?Sized> std::ops::Deref for Hot<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T: ?Sized> AsRef<T> for Hot<T> {
    fn as_ref(&self) -> &T {
        &self.data
    }
}

impl<T: ?Sized> From<Hot<T>> for Arc<T> {
    fn from(val: Hot<T>) -> Self {
        val.data
    }
}

#[cfg(feature = "serde")]
impl<T: ?Sized> serde::Serialize for Hot<T>
where
    T: serde::Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.get().serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de, T: ?Sized> serde::Deserialize<'de> for Hot<T>
where
    T: serde::Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let data = T::deserialize(deserializer)?;
        Ok(Self::new(data))
    }
}

#[test]
fn test() {
    let mut source = Hot::<str>::new("hello world");
    let mut hot = source.clone();
    source.update("hello hotsauce");
    assert!(hot.is_expired());
    hot.sync();
    assert!(!hot.is_expired());
    assert_eq!(hot.as_ref(), "hello hotsauce");
}
