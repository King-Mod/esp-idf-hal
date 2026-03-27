use core::marker::PhantomData;
use core::time::Duration;

use embedded_hal::i2c::{ErrorKind, NoAcknowledgeSource};

use esp_idf_sys::*;

use crate::delay::*;
use crate::gpio::*;
use crate::interrupt::InterruptType;
use crate::units::*;

pub use embedded_hal::i2c::Operation;

crate::embedded_hal_error!(
    I2cError,
    embedded_hal::i2c::Error,
    embedded_hal::i2c::ErrorKind
);

#[allow(unused)]
#[cfg(not(esp32c2))]
const APB_TICK_PERIOD_NS: u32 = 1_000_000_000 / APB_CLK_FREQ;

#[cfg(esp_idf_xtal_freq_48)]
#[allow(unused)]
const XTAL_TICK_PERIOD_NS: u32 = 1_000_000_000 / 48_000_000;
#[cfg(esp_idf_xtal_freq_40)]
#[allow(unused)]
const XTAL_TICK_PERIOD_NS: u32 = 1_000_000_000 / 40_000_000;
#[cfg(esp_idf_xtal_freq_32)]
#[allow(unused)]
const XTAL_TICK_PERIOD_NS: u32 = 1_000_000_000 / 32_000_000;
#[cfg(esp_idf_xtal_freq_26)]
#[allow(unused)]
const XTAL_TICK_PERIOD_NS: u32 = 1_000_000_000 / 26_000_000;

#[cfg(all(
    not(any(
        esp_idf_xtal_freq_48,
        esp_idf_xtal_freq_40,
        esp_idf_xtal_freq_32,
        esp_idf_xtal_freq_26
    )),
    any(esp32c5, esp32c61)
))]
const XTAL_TICK_PERIOD_NS: u32 = 1_000_000_000 / 48_000_000;
#[cfg(not(esp_idf_version_at_least_6_0_0))]
#[cfg(all(
    not(any(
        esp_idf_xtal_freq_48,
        esp_idf_xtal_freq_40,
        esp_idf_xtal_freq_32,
        esp_idf_xtal_freq_26
    )),
    not(any(esp32, esp32s2, esp32c2, esp32c5, esp32c61))
))]
const XTAL_TICK_PERIOD_NS: u32 = 1_000_000_000 / XTAL_CLK_FREQ;
// TODO: Below is probably not correct
#[cfg(esp_idf_version_at_least_6_0_0)]
#[cfg(all(
    not(any(
        esp_idf_xtal_freq_48,
        esp_idf_xtal_freq_40,
        esp_idf_xtal_freq_32,
        esp_idf_xtal_freq_26
    )),
    not(any(esp32, esp32s2, esp32c2, esp32c5, esp32c61))
))]
const XTAL_TICK_PERIOD_NS: u32 = 1_000_000_000 / 40_000_000;

#[derive(Copy, Clone, Debug)]
pub struct APBTickType(::core::ffi::c_int);
impl From<Duration> for APBTickType {
    #[cfg(any(esp32, esp32s2))]
    #[allow(clippy::manual_div_ceil)]
    fn from(duration: Duration) -> Self {
        APBTickType(
            ((duration.as_nanos() + APB_TICK_PERIOD_NS as u128 - 1) / APB_TICK_PERIOD_NS as u128)
                as ::core::ffi::c_int,
        )
    }
    #[cfg(not(any(esp32, esp32s2)))]
    /// Conversion for newer esp models, be aware, that the hardware can only represent 22 different values, values will be rounded to the next larger valid one. Calculation only valid for 40mhz clock source
    fn from(duration: Duration) -> Self {
        let target_ns = duration.as_nanos() as u64;
        let timeout_in_xtal_clock_cycles = target_ns / (XTAL_TICK_PERIOD_NS as u64);
        //ilog2 but with ceiling logic
        let register_value = timeout_in_xtal_clock_cycles.ilog2()
            + (if timeout_in_xtal_clock_cycles.leading_zeros()
                + timeout_in_xtal_clock_cycles.trailing_zeros()
                + 1
                < 64
            {
                1
            } else {
                0
            });
        if register_value <= 22 {
            return APBTickType(register_value as ::core::ffi::c_int);
        }
        //produce an error in the lower set_i2c_timeout, so the user is informed that the requested timeout is larger than the next valid one.
        APBTickType(32 as ::core::ffi::c_int)
    }
}

pub type I2cConfig = config::Config;
#[cfg(not(esp32c2))]
pub type I2cSlaveConfig = config::SlaveConfig;

/// I2C configuration
pub mod config {
    use enumset::EnumSet;

    use super::APBTickType;
    use crate::{interrupt::InterruptType, units::*};

    /// I2C Master configuration
    #[derive(Debug, Clone)]
    pub struct Config {
        pub baudrate: Hertz,
        pub sda_pullup_enabled: bool,
        pub scl_pullup_enabled: bool,
        pub timeout: Option<APBTickType>,
        pub intr_flags: EnumSet<InterruptType>,
    }

    impl Config {
        pub fn new() -> Self {
            Default::default()
        }

        #[must_use]
        pub fn baudrate(mut self, baudrate: Hertz) -> Self {
            self.baudrate = baudrate;
            self
        }

        #[must_use]
        pub fn sda_enable_pullup(mut self, enable: bool) -> Self {
            self.sda_pullup_enabled = enable;
            self
        }

        #[must_use]
        pub fn scl_enable_pullup(mut self, enable: bool) -> Self {
            self.scl_pullup_enabled = enable;
            self
        }

        #[must_use]
        pub fn timeout(mut self, timeout: APBTickType) -> Self {
            self.timeout = Some(timeout);
            self
        }

        #[must_use]
        pub fn intr_flags(mut self, flags: EnumSet<InterruptType>) -> Self {
            self.intr_flags = flags;
            self
        }
    }

    impl Default for Config {
        fn default() -> Self {
            Self {
                baudrate: Hertz(1_000_000),
                sda_pullup_enabled: true,
                scl_pullup_enabled: true,
                timeout: None,
                intr_flags: EnumSet::<InterruptType>::empty(),
            }
        }
    }

    /// I2C Slave configuration
    #[cfg(not(esp32c2))]
    #[derive(Debug, Clone)]
    pub struct SlaveConfig {
        pub sda_pullup_enabled: bool,
        pub scl_pullup_enabled: bool,
        pub rx_buf_len: usize,
        pub tx_buf_len: usize,
        pub intr_flags: EnumSet<InterruptType>,
    }

    #[cfg(not(esp32c2))]
    impl SlaveConfig {
        pub fn new() -> Self {
            Default::default()
        }

        #[must_use]
        pub fn sda_enable_pullup(mut self, enable: bool) -> Self {
            self.sda_pullup_enabled = enable;
            self
        }

        #[must_use]
        pub fn scl_enable_pullup(mut self, enable: bool) -> Self {
            self.scl_pullup_enabled = enable;
            self
        }

        #[must_use]
        pub fn rx_buffer_length(mut self, len: usize) -> Self {
            self.rx_buf_len = len;
            self
        }

        #[must_use]
        pub fn tx_buffer_length(mut self, len: usize) -> Self {
            self.tx_buf_len = len;
            self
        }

        #[must_use]
        pub fn intr_flags(mut self, flags: EnumSet<InterruptType>) -> Self {
            self.intr_flags = flags;
            self
        }
    }

    #[cfg(not(esp32c2))]
    impl Default for SlaveConfig {
        fn default() -> Self {
            Self {
                sda_pullup_enabled: true,
                scl_pullup_enabled: true,
                rx_buf_len: 0,
                tx_buf_len: 0,
                intr_flags: EnumSet::<InterruptType>::empty(),
            }
        }
    }
}

pub trait I2c: Send {
    fn port() -> i2c_port_t;
}

pub struct I2cDriver<'d> {
    i2c: u8,
    pub bus_handle: i2c_master_bus_handle_t,
    baudrate: u32,
    _p: PhantomData<&'d mut ()>,
}

impl<'d> I2cDriver<'d> {
    pub fn new<I2C: I2c + 'd>(
        _i2c: I2C,
        sda: impl InputPin + OutputPin + 'd,
        scl: impl InputPin + OutputPin + 'd,
        config: &config::Config,
    ) -> Result<Self, EspError> {
        // i2c_config_t documentation says that clock speed must be no higher than 1 MHz
        if config.baudrate > 1.MHz().into() {
            return Err(EspError::from_infallible::<ESP_ERR_INVALID_ARG>());
        }

        // Use the new I2C master bus API
        let bus_config = i2c_master_bus_config_t {
            i2c_port: I2C::port() as i32,
            sda_io_num: sda.pin() as _,
            scl_io_num: scl.pin() as _,
            __bindgen_anon_1: i2c_master_bus_config_t__bindgen_ty_1 {
                clk_source: soc_periph_i2c_clk_src_t_I2C_CLK_SRC_DEFAULT,
            },
            glitch_ignore_cnt: 7,
            intr_priority: 0,
            trans_queue_depth: 0,
            flags: i2c_master_bus_config_t__bindgen_ty_2 {
                _bitfield_1: i2c_master_bus_config_t__bindgen_ty_2::new_bitfield_1(
                    if config.sda_pullup_enabled || config.scl_pullup_enabled {
                        1
                    } else {
                        0
                    },
                    0, // allow_pd
                ),
                ..Default::default()
            },
        };

        let mut bus_handle: i2c_master_bus_handle_t = core::ptr::null_mut();
        esp!(unsafe { i2c_new_master_bus(&bus_config, &mut bus_handle) })?;

        Ok(I2cDriver {
            i2c: I2C::port() as _,
            bus_handle,
            baudrate: config.baudrate.into(),
            _p: PhantomData,
        })
    }

    // Helper to create a temporary device handle for a transaction
    fn create_device_handle(&self, addr: u8) -> Result<i2c_master_dev_handle_t, EspError> {
        let dev_config = i2c_device_config_t {
            dev_addr_length: i2c_addr_bit_len_t_I2C_ADDR_BIT_LEN_7,
            device_address: addr as u16,
            scl_speed_hz: self.baudrate,
            scl_wait_us: 0,
            flags: i2c_device_config_t__bindgen_ty_1 {
                _bitfield_1: i2c_device_config_t__bindgen_ty_1::new_bitfield_1(0),
                ..Default::default()
            },
        };

        let mut dev_handle: i2c_master_dev_handle_t = core::ptr::null_mut();
        esp!(unsafe { i2c_master_bus_add_device(self.bus_handle, &dev_config, &mut dev_handle) })?;
        Ok(dev_handle)
    }

    pub fn read(
        &mut self,
        addr: u8,
        buffer: &mut [u8],
        timeout: TickType_t,
    ) -> Result<(), EspError> {
        if buffer.is_empty() {
            return Ok(());
        }

        let dev_handle = self.create_device_handle(addr)?;
        let result = esp!(unsafe {
            i2c_master_receive(
                dev_handle,
                buffer.as_mut_ptr(),
                buffer.len(),
                timeout as i32,
            )
        });
        unsafe { i2c_master_bus_rm_device(dev_handle) };
        result
    }

    pub fn write(&mut self, addr: u8, bytes: &[u8], timeout: TickType_t) -> Result<(), EspError> {
        if bytes.is_empty() {
            return Ok(());
        }

        let dev_handle = self.create_device_handle(addr)?;
        let result = esp!(unsafe {
            i2c_master_transmit(dev_handle, bytes.as_ptr(), bytes.len(), timeout as i32)
        });
        unsafe { i2c_master_bus_rm_device(dev_handle) };
        result
    }

    pub fn write_read(
        &mut self,
        addr: u8,
        bytes: &[u8],
        buffer: &mut [u8],
        timeout: TickType_t,
    ) -> Result<(), EspError> {
        let dev_handle = self.create_device_handle(addr)?;
        let result = esp!(unsafe {
            i2c_master_transmit_receive(
                dev_handle,
                bytes.as_ptr(),
                bytes.len(),
                buffer.as_mut_ptr(),
                buffer.len(),
                timeout as i32,
            )
        });
        unsafe { i2c_master_bus_rm_device(dev_handle) };
        result
    }

    pub fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
        timeout: TickType_t,
    ) -> Result<(), EspError> {
        let dev_handle = self.create_device_handle(address)?;

        // Process operations sequentially
        for operation in operations.iter_mut() {
            let result = match operation {
                Operation::Read(buf) => {
                    if buf.is_empty() {
                        Ok(())
                    } else {
                        esp!(unsafe {
                            i2c_master_receive(
                                dev_handle,
                                buf.as_mut_ptr(),
                                buf.len(),
                                timeout as i32,
                            )
                        })
                    }
                }
                Operation::Write(buf) => {
                    if buf.is_empty() {
                        Ok(())
                    } else {
                        esp!(unsafe {
                            i2c_master_transmit(dev_handle, buf.as_ptr(), buf.len(), timeout as i32)
                        })
                    }
                }
            };

            if let Err(e) = result {
                unsafe { i2c_master_bus_rm_device(dev_handle) };
                return Err(e);
            }
        }

        unsafe { i2c_master_bus_rm_device(dev_handle) };
        Ok(())
    }

    pub fn port(&self) -> i2c_port_t {
        self.i2c as _
    }
}

impl Drop for I2cDriver<'_> {
    fn drop(&mut self) {
        unsafe { i2c_del_master_bus(self.bus_handle) };
    }
}

unsafe impl Send for I2cDriver<'_> {}

impl embedded_hal_0_2::blocking::i2c::Read for I2cDriver<'_> {
    type Error = I2cError;

    fn read(&mut self, addr: u8, buffer: &mut [u8]) -> Result<(), Self::Error> {
        I2cDriver::read(self, addr, buffer, BLOCK).map_err(to_i2c_err)
    }
}

impl embedded_hal_0_2::blocking::i2c::Write for I2cDriver<'_> {
    type Error = I2cError;

    fn write(&mut self, addr: u8, bytes: &[u8]) -> Result<(), Self::Error> {
        I2cDriver::write(self, addr, bytes, BLOCK).map_err(to_i2c_err)
    }
}

impl embedded_hal_0_2::blocking::i2c::WriteRead for I2cDriver<'_> {
    type Error = I2cError;

    fn write_read(&mut self, addr: u8, bytes: &[u8], buffer: &mut [u8]) -> Result<(), Self::Error> {
        I2cDriver::write_read(self, addr, bytes, buffer, BLOCK).map_err(to_i2c_err)
    }
}

impl embedded_hal::i2c::ErrorType for I2cDriver<'_> {
    type Error = I2cError;
}

impl embedded_hal::i2c::I2c<embedded_hal::i2c::SevenBitAddress> for I2cDriver<'_> {
    fn read(&mut self, addr: u8, buffer: &mut [u8]) -> Result<(), Self::Error> {
        I2cDriver::read(self, addr, buffer, BLOCK).map_err(to_i2c_err)
    }

    fn write(&mut self, addr: u8, bytes: &[u8]) -> Result<(), Self::Error> {
        I2cDriver::write(self, addr, bytes, BLOCK).map_err(to_i2c_err)
    }

    fn write_read(&mut self, addr: u8, bytes: &[u8], buffer: &mut [u8]) -> Result<(), Self::Error> {
        I2cDriver::write_read(self, addr, bytes, buffer, BLOCK).map_err(to_i2c_err)
    }

    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [embedded_hal::i2c::Operation<'_>],
    ) -> Result<(), Self::Error> {
        I2cDriver::transaction(self, address, operations, BLOCK).map_err(to_i2c_err)
    }
}

fn to_i2c_err(err: EspError) -> I2cError {
    if err.code() == ESP_FAIL {
        I2cError::new(ErrorKind::NoAcknowledge(NoAcknowledgeSource::Unknown), err)
    } else {
        I2cError::other(err)
    }
}

#[cfg(not(esp32c2))]
pub struct I2cSlaveDriver<'d> {
    i2c: u8,
    handle: i2c_slave_dev_handle_t,
    rx_queue: crate::sys::QueueHandle_t,
    _p: PhantomData<&'d mut ()>,
}

#[cfg(not(esp32c2))]
unsafe impl Send for I2cSlaveDriver<'_> {}

#[cfg(not(esp32c2))]
unsafe extern "C" fn i2c_slave_rx_done_callback(
    _channel: i2c_slave_dev_handle_t,
    edata: *const i2c_slave_rx_done_event_data_t,
    user_data: *mut core::ffi::c_void,
) -> bool {
    if !edata.is_null() && !user_data.is_null() {
        let queue = user_data as crate::sys::QueueHandle_t;
        let mut high_task_wakeup: crate::sys::BaseType_t = 0;
        crate::sys::xQueueGenericSendFromISR(
            queue,
            edata as *const core::ffi::c_void,
            &mut high_task_wakeup,
            0, // queueSEND_TO_BACK
        );
        return high_task_wakeup != 0;
    }
    false
}

#[cfg(not(esp32c2))]
impl<'d> I2cSlaveDriver<'d> {
    pub fn new<I2C: I2c + 'd>(
        _i2c: I2C,
        sda: impl InputPin + OutputPin + 'd,
        scl: impl InputPin + OutputPin + 'd,
        slave_addr: u8,
        config: &config::SlaveConfig,
    ) -> Result<Self, EspError> {
        // Use the new I2C slave device API
        let sys_config = i2c_slave_config_t {
            i2c_port: I2C::port() as i32,
            sda_io_num: sda.pin() as _,
            scl_io_num: scl.pin() as _,
            clk_source: soc_periph_i2c_clk_src_t_I2C_CLK_SRC_DEFAULT,
            slave_addr: slave_addr as u16,
            addr_bit_len: i2c_addr_bit_len_t_I2C_ADDR_BIT_LEN_7,
            send_buf_depth: if config.tx_buf_len > 0 {
                config.tx_buf_len as u32
            } else {
                256
            },
            flags: i2c_slave_config_t__bindgen_ty_1 {
                _bitfield_1: i2c_slave_config_t__bindgen_ty_1::new_bitfield_1(
                    0, // stretch_en
                    0, // broadcast_en
                    0, // access_ram_en
                    0, // slave_unmatch_en
                    0, // allow_pd
                ),
                ..Default::default()
            },
            intr_priority: 0,
        };

        let mut handle: i2c_slave_dev_handle_t = core::ptr::null_mut();
        esp!(unsafe { i2c_new_slave_device(&sys_config, &mut handle) })?;

        // Create queue for receiving callback notifications
        let rx_queue = unsafe {
            crate::sys::xQueueGenericCreate(
                1,
                core::mem::size_of::<i2c_slave_rx_done_event_data_t>() as u32,
                0, // queueQUEUE_TYPE_BASE
            )
        };

        if rx_queue.is_null() {
            unsafe { i2c_del_slave_device(handle) };
            return Err(EspError::from_infallible::<ESP_ERR_NO_MEM>());
        }

        // Register the receive callback
        let cbs = i2c_slave_event_callbacks_t {
            on_stretch_occur: None,
            on_recv_done: Some(i2c_slave_rx_done_callback),
        };

        if let Err(e) = esp!(unsafe {
            i2c_slave_register_event_callbacks(handle, &cbs, rx_queue as *mut core::ffi::c_void)
        }) {
            unsafe {
                crate::sys::vQueueDelete(rx_queue);
                i2c_del_slave_device(handle);
            }
            return Err(e);
        }

        Ok(Self {
            i2c: I2C::port() as _,
            handle,
            rx_queue,
            _p: PhantomData,
        })
    }

    pub fn read(&mut self, buffer: &mut [u8], timeout: TickType_t) -> Result<usize, EspError> {
        let buffer_len = buffer.len();

        // Start non-blocking receive operation
        esp!(unsafe { i2c_slave_receive(self.handle, buffer.as_mut_ptr(), buffer_len) })?;

        // Wait for the callback to signal completion
        let mut rx_data: i2c_slave_rx_done_event_data_t = unsafe { core::mem::zeroed() };
        let received = unsafe {
            crate::sys::xQueueReceive(
                self.rx_queue,
                &mut rx_data as *mut _ as *mut core::ffi::c_void,
                timeout,
            )
        };

        if received != 0 {
            // Successfully received data
            // The buffer pointer should match our buffer, and the length is what we requested
            // In the new I2C slave API, the receive completes when buffer_len bytes are received
            Ok(buffer_len)
        } else {
            Err(EspError::from_infallible::<ESP_ERR_TIMEOUT>())
        }
    }

    pub fn write(&mut self, bytes: &[u8], timeout: TickType_t) -> Result<usize, EspError> {
        // Use the new non-blocking transmit API which returns immediately
        esp!(unsafe {
            i2c_slave_transmit(
                self.handle,
                bytes.as_ptr(),
                bytes.len() as i32,
                timeout as i32,
            )
        })?;

        Ok(bytes.len())
    }

    pub fn port(&self) -> i2c_port_t {
        self.i2c as _
    }
}

#[cfg(not(esp32c2))]
impl Drop for I2cSlaveDriver<'_> {
    fn drop(&mut self) {
        unsafe {
            // Delete the queue
            crate::sys::vQueueDelete(self.rx_queue);
            // Delete the I2C slave device
            i2c_del_slave_device(self.handle);
        }
    }
}

macro_rules! impl_i2c {
    ($i2c:ident: $port:expr) => {
        crate::impl_peripheral!($i2c);

        impl I2c for $i2c<'_> {
            #[inline(always)]
            fn port() -> i2c_port_t {
                $port
            }
        }
    };
}

impl_i2c!(I2C0: 0);
#[cfg(not(any(esp32c3, esp32c2, esp32c6)))]
impl_i2c!(I2C1: 1);
