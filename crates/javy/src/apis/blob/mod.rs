use crate::{
    hold, hold_and_release,
    quickjs::{prelude::MutFn, context::EvalOptions, ArrayBuffer, Ctx, Function, String as JSString, TypedArray, Value},
    to_js_error, val_to_string, Args,
};
use anyhow::{anyhow, Error, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Internal blob storage with reference counting
#[derive(Debug, Clone)]
struct BlobData {
    data: Vec<u8>,
    mime_type: String,
}

/// Global blob storage to handle blob references
type BlobStorage = HashMap<u32, BlobData>;
static BLOB_STORAGE: OnceLock<Arc<Mutex<BlobStorage>>> = OnceLock::new();
static NEXT_BLOB_ID: OnceLock<Arc<Mutex<u32>>> = OnceLock::new();

fn get_blob_storage() -> &'static Arc<Mutex<BlobStorage>> {
    BLOB_STORAGE.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

fn get_next_blob_id() -> u32 {
    let counter = NEXT_BLOB_ID.get_or_init(|| Arc::new(Mutex::new(1)));
    let mut id = counter.lock().unwrap();
    let current = *id;
    *id += 1;
    current
}

/// Register Blob and File helper functions and JavaScript classes
pub(crate) fn register(this: Ctx<'_>) -> Result<()> {
    let globals = this.globals();

    // Register helper functions
    globals.set(
        "__javy_blob_create",
        Function::new(this.clone(), MutFn::new(move |cx, args| {
            let (cx, args) = hold_and_release!(cx, args);
            blob_create(hold!(cx.clone(), args)).map_err(|e| to_js_error(cx, e))
        })),
    )?;

    globals.set(
        "__javy_blob_get_property",
        Function::new(this.clone(), MutFn::new(move |cx, args| {
            let (cx, args) = hold_and_release!(cx, args);
            blob_get_property(hold!(cx.clone(), args)).map_err(|e| to_js_error(cx, e))
        })),
    )?;

    globals.set(
        "__javy_blob_array_buffer",
        Function::new(this.clone(), MutFn::new(move |cx, args| {
            let (cx, args) = hold_and_release!(cx, args);
            blob_array_buffer(hold!(cx.clone(), args)).map_err(|e| to_js_error(cx, e))
        })),
    )?;

    globals.set(
        "__javy_blob_bytes",
        Function::new(this.clone(), MutFn::new(move |cx, args| {
            let (cx, args) = hold_and_release!(cx, args);
            blob_bytes(hold!(cx.clone(), args)).map_err(|e| to_js_error(cx, e))
        })),
    )?;

    globals.set(
        "__javy_blob_text",
        Function::new(this.clone(), MutFn::new(move |cx, args| {
            let (cx, args) = hold_and_release!(cx, args);
            blob_text(hold!(cx.clone(), args)).map_err(|e| to_js_error(cx, e))
        })),
    )?;

    globals.set(
        "__javy_blob_slice",
        Function::new(this.clone(), MutFn::new(move |cx, args| {
            let (cx, args) = hold_and_release!(cx, args);
            blob_slice(hold!(cx.clone(), args)).map_err(|e| to_js_error(cx, e))
        })),
    )?;

    globals.set(
        "__javy_file_create",
        Function::new(this.clone(), MutFn::new(move |cx, args| {
            let (cx, args) = hold_and_release!(cx, args);
            file_create(hold!(cx.clone(), args)).map_err(|e| to_js_error(cx, e))
        })),
    )?;

    // Load the JavaScript implementation
    let mut opts = EvalOptions::default();
    opts.strict = false;
    this.eval_with_options::<(), _>(include_str!("./blob.js"), opts)?;

    Ok::<_, Error>(())
}

/// Create a new blob and return its ID
fn blob_create<'js>(args: Args<'js>) -> Result<Value<'js>> {
    let (ctx, args) = args.release();
    let args = args.into_inner();

    // Parse blobParts (first argument, defaults to empty array)
    let blob_parts = if args.is_empty() {
        vec![]
    } else {
        parse_blob_parts(&ctx, args[0].clone())?
    };

    // Parse options (second argument, defaults to empty object)
    let options = if args.len() > 1 {
        parse_blob_options(&ctx, args[1].clone())?
    } else {
        BlobOptions::default()
    };

    // Concatenate all blob parts
    let mut data = Vec::new();
    for part in blob_parts {
        data.extend_from_slice(&part);
    }

    // Create blob and store it
    let id = get_next_blob_id();
    let blob_data = BlobData { 
        data, 
        mime_type: options.mime_type 
    };
    
    let storage = get_blob_storage();
    storage.lock().unwrap().insert(id, blob_data);

    Ok(Value::new_number(ctx, id as f64))
}

/// Create a new file and return its ID  
fn file_create<'js>(args: Args<'js>) -> Result<Value<'js>> {
    let (ctx, args) = args.release();
    let args = args.into_inner();

    if args.len() < 2 {
        return Err(anyhow!("File constructor requires at least 2 arguments"));
    }

    // Parse fileBits (first argument)
    let file_bits = parse_blob_parts(&ctx, args[0].clone())?;
    
    // Parse fileName (second argument)
    let _file_name = val_to_string(&ctx, args[1].clone())?;

    // Parse options (third argument, optional)
    let options = if args.len() > 2 {
        parse_file_options(&ctx, args[2].clone())?
    } else {
        FileOptions::default()
    };

    // Concatenate all file bits
    let mut data = Vec::new();
    for part in file_bits {
        data.extend_from_slice(&part);
    }

    // Create file blob and store it (files are just blobs with metadata)
    let id = get_next_blob_id();
    let blob_data = BlobData { 
        data, 
        mime_type: options.mime_type 
    };
    
    let storage = get_blob_storage();
    storage.lock().unwrap().insert(id, blob_data);

    Ok(Value::new_number(ctx, id as f64))
}

/// Get a property of a blob by ID
fn blob_get_property<'js>(args: Args<'js>) -> Result<Value<'js>> {
    let (ctx, args) = args.release();
    let args = args.into_inner();

    if args.len() < 2 {
        return Err(anyhow!("blob_get_property requires 2 arguments"));
    }

    let blob_id = args[0].as_number().ok_or_else(|| anyhow!("Blob ID must be a number"))? as u32;
    let property = val_to_string(&ctx, args[1].clone())?;

    let storage = get_blob_storage();
    let storage_guard = storage.lock().unwrap();
    
    if let Some(blob_data) = storage_guard.get(&blob_id) {
        match property.as_str() {
            "size" => Ok(Value::new_number(ctx, blob_data.data.len() as f64)),
            "type" => {
                let js_string = JSString::from_str(ctx.clone(), &blob_data.mime_type)?;
                Ok(Value::from_string(js_string))
            }
            _ => Err(anyhow!("Unknown property: {}", property))
        }
    } else {
        Err(anyhow!("Invalid blob ID: {}", blob_id))
    }
}

/// Get ArrayBuffer from blob by ID
fn blob_array_buffer<'js>(args: Args<'js>) -> Result<Value<'js>> {
    let (ctx, args) = args.release();
    let args = args.into_inner();

    if args.is_empty() {
        return Err(anyhow!("blob_array_buffer requires 1 argument"));
    }

    let blob_id = args[0].as_number().ok_or_else(|| anyhow!("Blob ID must be a number"))? as u32;

    let storage = get_blob_storage();
    let storage_guard = storage.lock().unwrap();
    
    if let Some(blob_data) = storage_guard.get(&blob_id) {
        let array_buffer = ArrayBuffer::new(ctx.clone(), blob_data.data.clone())?;
        Ok(array_buffer.into_value())
    } else {
        let empty_buffer = ArrayBuffer::new(ctx.clone(), Vec::<u8>::new())?;
        Ok(empty_buffer.into_value())
    }
}

/// Get Uint8Array from blob by ID
fn blob_bytes<'js>(args: Args<'js>) -> Result<Value<'js>> {
    let (ctx, args) = args.release();
    let args = args.into_inner();

    if args.is_empty() {
        return Err(anyhow!("blob_bytes requires 1 argument"));
    }

    let blob_id = args[0].as_number().ok_or_else(|| anyhow!("Blob ID must be a number"))? as u32;

    let storage = get_blob_storage();
    let storage_guard = storage.lock().unwrap();
    
    if let Some(blob_data) = storage_guard.get(&blob_id) {
        let typed_array: TypedArray<u8> = TypedArray::new(ctx.clone(), blob_data.data.clone())?;
        Ok(typed_array.as_value().to_owned())
    } else {
        let empty_array: TypedArray<u8> = TypedArray::new(ctx.clone(), Vec::<u8>::new())?;
        Ok(empty_array.as_value().to_owned())
    }
}

/// Get text content from blob by ID
fn blob_text<'js>(args: Args<'js>) -> Result<Value<'js>> {
    let (ctx, args) = args.release();
    let args = args.into_inner();

    if args.is_empty() {
        return Err(anyhow!("blob_text requires 1 argument"));
    }

    let blob_id = args[0].as_number().ok_or_else(|| anyhow!("Blob ID must be a number"))? as u32;

    let storage = get_blob_storage();
    let storage_guard = storage.lock().unwrap();
    
    if let Some(blob_data) = storage_guard.get(&blob_id) {
        let text = String::from_utf8_lossy(&blob_data.data);
        let js_string = JSString::from_str(ctx.clone(), &text)?;
        Ok(Value::from_string(js_string))
    } else {
        let js_string = JSString::from_str(ctx.clone(), "")?;
        Ok(Value::from_string(js_string))
    }
}

/// Slice a blob and return new blob ID
fn blob_slice<'js>(args: Args<'js>) -> Result<Value<'js>> {
    let (ctx, args) = args.release();
    let args = args.into_inner();

    if args.is_empty() {
        return Err(anyhow!("blob_slice requires at least 1 argument"));
    }

    let blob_id = args[0].as_number().ok_or_else(|| anyhow!("Blob ID must be a number"))? as u32;

    let start = if args.len() > 1 && !args[1].is_undefined() {
        Some(args[1].as_number().unwrap_or(0.0) as i64)
    } else {
        None
    };

    let end = if args.len() > 2 && !args[2].is_undefined() {
        Some(args[2].as_number().unwrap_or(0.0) as i64)
    } else {
        None
    };

    let content_type = if args.len() > 3 && !args[3].is_undefined() {
        Some(val_to_string(&ctx, args[3].clone())?)
    } else {
        None
    };

    let storage = get_blob_storage();
    let storage_guard = storage.lock().unwrap();
    
    if let Some(blob_data) = storage_guard.get(&blob_id) {
        let len = blob_data.data.len() as i64;
        
        // Calculate actual start and end positions
        let actual_start = match start {
            Some(s) if s < 0 => (len + s).max(0) as usize,
            Some(s) => s.min(len) as usize,
            None => 0,
        };
        
        let actual_end = match end {
            Some(e) if e < 0 => (len + e).max(0) as usize,
            Some(e) => e.min(len) as usize,
            None => len as usize,
        };
        
        let actual_end = actual_end.max(actual_start);
        
        let sliced_data = if actual_start >= blob_data.data.len() {
            Vec::new()
        } else {
            blob_data.data[actual_start..actual_end.min(blob_data.data.len())].to_vec()
        };
        
        // Create new blob with sliced data
        let new_id = get_next_blob_id();
        let new_mime_type = content_type.unwrap_or_default();
        let new_blob_data = BlobData { 
            data: sliced_data, 
            mime_type: new_mime_type 
        };
        
        drop(storage_guard); // Release the lock before acquiring it again
        let storage = get_blob_storage();
        storage.lock().unwrap().insert(new_id, new_blob_data);

        Ok(Value::new_number(ctx, new_id as f64))
    } else {
        // Return empty blob on error
        let new_id = get_next_blob_id();
        let empty_blob_data = BlobData { 
            data: Vec::new(), 
            mime_type: String::new() 
        };
        
        drop(storage_guard);
        let storage = get_blob_storage();
        storage.lock().unwrap().insert(new_id, empty_blob_data);

        Ok(Value::new_number(ctx, new_id as f64))
    }
}

#[derive(Default)]
struct BlobOptions {
    mime_type: String,
    endings: String, // "transparent" or "native"
}

#[derive(Default)]
struct FileOptions {
    mime_type: String,
    endings: String,
    last_modified: Option<u64>,
}

fn parse_blob_options<'a>(ctx: &Ctx<'a>, value: Value<'a>) -> Result<BlobOptions> {
    if let Some(obj) = value.as_object() {
        let mut options = BlobOptions::default();

        if let Ok(type_val) = obj.get::<_, Value>("type") {
            options.mime_type = val_to_string(ctx, type_val)?;
        }

        if let Ok(endings_val) = obj.get::<_, Value>("endings") {
            let endings = val_to_string(ctx, endings_val)?;
            if endings == "native" || endings == "transparent" {
                options.endings = endings;
            }
        }

        Ok(options)
    } else {
        Ok(BlobOptions::default())
    }
}

fn parse_file_options<'a>(ctx: &Ctx<'a>, value: Value<'a>) -> Result<FileOptions> {
    if let Some(obj) = value.as_object() {
        let mut options = FileOptions::default();

        if let Ok(type_val) = obj.get::<_, Value>("type") {
            options.mime_type = val_to_string(ctx, type_val)?;
        }

        if let Ok(endings_val) = obj.get::<_, Value>("endings") {
            let endings = val_to_string(ctx, endings_val)?;
            if endings == "native" || endings == "transparent" {
                options.endings = endings;
            }
        }

        if let Ok(last_modified_val) = obj.get::<_, Value>("lastModified") {
            if let Some(num) = last_modified_val.as_number() {
                options.last_modified = Some(num.max(0.0) as u64);
            }
        }

        Ok(options)
    } else {
        Ok(FileOptions::default())
    }
}

fn parse_blob_parts<'a>(ctx: &Ctx<'a>, value: Value<'a>) -> Result<Vec<Vec<u8>>> {
    let mut parts = Vec::new();

    if value.is_array() {
        if let Some(array) = value.as_object() {
            let len = array.len();
            for i in 0..len {
                if let Ok(item) = array.get::<_, Value>(i as u32) {
                    let part_data = convert_to_bytes(ctx, item)?;
                    parts.push(part_data);
                }
            }
        }
    } else {
        // Single item, treat as array with one element
        let part_data = convert_to_bytes(ctx, value)?;
        parts.push(part_data);
    }

    Ok(parts)
}

fn convert_to_bytes<'a>(ctx: &Ctx<'a>, value: Value<'a>) -> Result<Vec<u8>> {
    if value.is_string() {
        let s = val_to_string(ctx, value)?;
        Ok(s.into_bytes())
    } else if let Some(obj) = value.as_object() {
        if let Some(array_buffer) = obj.as_array_buffer() {
            if let Some(bytes) = array_buffer.as_bytes() {
                Ok(bytes.to_vec())
            } else {
                Err(anyhow!("Could not get bytes from ArrayBuffer"))
            }
        } else {
            // Check if this is a TypedArray by checking if it has the right properties
            if let (Ok(constructor), Ok(length)) = (obj.get::<_, Value>("constructor"), obj.get::<_, Value>("length")) {
                if let Some(constructor_obj) = constructor.as_object() {
                    if let Ok(name) = constructor_obj.get::<_, Value>("name") {
                        let name_str = val_to_string(ctx, name).unwrap_or_default();
                        if name_str == "Uint8Array" {
                            // This is a Uint8Array, extract the bytes
                            if let Some(length_num) = length.as_number() {
                                let len = length_num as usize;
                                let mut bytes = Vec::with_capacity(len);
                                for i in 0..len {
                                    if let Ok(byte_val) = obj.get::<_, Value>(i as u32) {
                                        if let Some(byte_num) = byte_val.as_number() {
                                            bytes.push(byte_num as u8);
                                        }
                                    }
                                }
                                return Ok(bytes);
                            }
                        }
                    }
                }
            }
            
            // Try TypedArray approach as backup
            if let Ok(typed_array) = TypedArray::<u8>::from_object(obj.clone()) {
                let bytes: &[u8] = typed_array.as_ref();
                Ok(bytes.to_vec())
            } else {
                // Try to convert to string as fallback
                let s = val_to_string(ctx, value)?;
                Ok(s.into_bytes())
            }
        }
    } else {
        // Try to convert to string as fallback
        let s = val_to_string(ctx, value)?;
        Ok(s.into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config, Runtime};
    use anyhow::Error;

    #[test]
    fn test_register() -> Result<()> {
        let config = Config::default();
        let runtime = Runtime::new(config)?;
        runtime.context().with(|cx| {
            register(cx.clone())?;
            
            // Check that Blob is available
            let result: Value = cx.eval("typeof Blob")?;
            let type_str = val_to_string(&cx, result)?;
            assert_eq!(type_str, "function");
            
            // Check that File is available
            let result: Value = cx.eval("typeof File")?;
            let type_str = val_to_string(&cx, result)?;
            assert_eq!(type_str, "function");
            
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_blob_constructor_basic() -> Result<()> {
        let config = Config::default();
        let runtime = Runtime::new(config)?;
        runtime.context().with(|cx| {
            register(cx.clone())?;
            
            // Test empty blob
            let result: Value = cx.eval("new Blob()")?;
            assert!(result.is_object());
            
            // Test blob with string content
            let result: Value = cx.eval("new Blob(['hello world'])")?;
            assert!(result.is_object());
            
            // Test blob with options
            let result: Value = cx.eval("new Blob(['test'], { type: 'text/plain' })")?;
            assert!(result.is_object());
            
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_blob_properties() -> Result<()> {
        let config = Config::default();
        let runtime = Runtime::new(config)?;
        runtime.context().with(|cx| {
            register(cx.clone())?;
            
            // Test size property
            let result: Value = cx.eval("new Blob(['hello']).size")?;
            assert_eq!(result.as_number().unwrap() as u64, 5);
            
            // Test type property
            let result: Value = cx.eval("new Blob(['test'], { type: 'text/plain' }).type")?;
            let type_str = val_to_string(&cx, result)?;
            assert_eq!(type_str, "text/plain");
            
            // Test empty type
            let result: Value = cx.eval("new Blob(['test']).type")?;
            let type_str = val_to_string(&cx, result)?;
            assert_eq!(type_str, "");
            
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_blob_text_method() -> Result<()> {
        let config = Config::default();
        let runtime = Runtime::new(config)?;
        runtime.context().with(|cx| {
            register(cx.clone())?;
            
            // Test text method
            let result: Value = cx.eval("new Blob(['hello world']).text()")?;
            let text = val_to_string(&cx, result)?;
            assert_eq!(text, "hello world");
            
            // Test empty blob text
            let result: Value = cx.eval("new Blob().text()")?;
            let text = val_to_string(&cx, result)?;
            assert_eq!(text, "");
            
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_blob_slice_method() -> Result<()> {
        let config = Config::default();
        let runtime = Runtime::new(config)?;
        runtime.context().with(|cx| {
            register(cx.clone())?;
            
            // Test basic slice
            let result: Value = cx.eval("new Blob(['hello world']).slice(0, 5).text()")?;
            let text = val_to_string(&cx, result)?;
            assert_eq!(text, "hello");
            
            // Test slice with negative start
            let result: Value = cx.eval("new Blob(['hello world']).slice(-5).text()")?;
            let text = val_to_string(&cx, result)?;
            assert_eq!(text, "world");
            
            // Test slice with content type
            let result: Value = cx.eval("new Blob(['test']).slice(0, 2, 'text/plain').type")?;
            let type_str = val_to_string(&cx, result)?;
            assert_eq!(type_str, "text/plain");
            
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_file_constructor() -> Result<()> {
        let config = Config::default();
        let runtime = Runtime::new(config)?;
        runtime.context().with(|cx| {
            register(cx.clone())?;
            
            // Test basic file
            let result: Value = cx.eval("new File(['content'], 'test.txt')")?;
            assert!(result.is_object());
            
            // Test file with options
            let result: Value = cx.eval("new File(['content'], 'test.txt', { type: 'text/plain' })")?;
            assert!(result.is_object());
            
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_file_properties() -> Result<()> {
        let config = Config::default();
        let runtime = Runtime::new(config)?;
        runtime.context().with(|cx| {
            register(cx.clone())?;
            
            // Test name property
            let result: Value = cx.eval("new File(['content'], 'test.txt').name")?;
            let name = val_to_string(&cx, result)?;
            assert_eq!(name, "test.txt");
            
            // Test inherited size property
            let result: Value = cx.eval("new File(['hello'], 'test.txt').size")?;
            assert_eq!(result.as_number().unwrap() as u64, 5);
            
            // Test inherited type property
            let result: Value = cx.eval("new File(['content'], 'test.txt', { type: 'text/plain' }).type")?;
            let type_str = val_to_string(&cx, result)?;
            assert_eq!(type_str, "text/plain");
            
            // Test lastModified property exists
            let result: Value = cx.eval("typeof new File(['content'], 'test.txt').lastModified")?;
            let type_str = val_to_string(&cx, result)?;
            assert_eq!(type_str, "number");
            
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_file_inherited_methods() -> Result<()> {
        let config = Config::default();
        let runtime = Runtime::new(config)?;
        runtime.context().with(|cx| {
            register(cx.clone())?;
            
            // Test inherited text method
            let result: Value = cx.eval("new File(['hello world'], 'test.txt').text()")?;
            let text = val_to_string(&cx, result)?;
            assert_eq!(text, "hello world");
            
            // Test inherited slice method
            let result: Value = cx.eval("new File(['hello world'], 'test.txt').slice(0, 5).text()")?;
            let text = val_to_string(&cx, result)?;
            assert_eq!(text, "hello");
            
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_blob_concatenation() -> Result<()> {
        let config = Config::default();
        let runtime = Runtime::new(config)?;
        runtime.context().with(|cx| {
            register(cx.clone())?;
            
            // Test multiple string parts
            let result: Value = cx.eval("new Blob(['hello', ' ', 'world']).text()")?;
            let text = val_to_string(&cx, result)?;
            assert_eq!(text, "hello world");
            
            // Test size calculation
            let result: Value = cx.eval("new Blob(['hello', ' ', 'world']).size")?;
            assert_eq!(result.as_number().unwrap() as u64, 11);
            
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_edge_cases() -> Result<()> {
        let config = Config::default();
        let runtime = Runtime::new(config)?;
        runtime.context().with(|cx| {
            register(cx.clone())?;
            
            // Test slice beyond bounds
            let result: Value = cx.eval("new Blob(['hello']).slice(10, 20).size")?;
            assert_eq!(result.as_number().unwrap() as u64, 0);
            
            // Test slice with end before start
            let result: Value = cx.eval("new Blob(['hello']).slice(3, 1).size")?;
            assert_eq!(result.as_number().unwrap() as u64, 0);
            
            // Test File constructor with missing arguments
            let result = cx.eval::<Value, _>("new File(['content'])");
            assert!(result.is_err());
            
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_debug_blob_constructor() -> Result<()> {
        let config = Config::default();
        let runtime = Runtime::new(config)?;
        runtime.context().with(|cx| {
            register(cx.clone())?;
            
            // Debug test: Check if Blob exists and is callable
            println!("Testing Blob existence...");
            let result: Value = cx.eval("typeof Blob")?;
            let type_str = val_to_string(&cx, result)?;
            println!("Blob type: {}", type_str);
            
            // Try to call as function first
            println!("Testing Blob as function...");
            let result = cx.eval::<Value, _>("Blob()");
            match result {
                Ok(val) => {
                    println!("Blob() succeeded: {:?}", val.type_name());
                }
                Err(e) => {
                    println!("Blob() failed: {}", e);
                }
            }
            
            // Try to call with new
            println!("Testing new Blob...");
            let result = cx.eval::<Value, _>("new Blob()");
            match result {
                Ok(val) => {
                    println!("new Blob() succeeded: {:?}", val.type_name());
                }
                Err(e) => {
                    println!("new Blob() failed: {}", e);
                }
            }
            
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_blob_integration() -> Result<()> {
        let config = Config::default();
        let runtime = Runtime::new(config)?;
        runtime.context().with(|cx| {
            register(cx.clone())?;
            
            // Test comprehensive integration without console dependencies
            let test_script = r#"
// Test 1: Basic Blob creation and properties
const blob1 = new Blob(['Hello, ', 'World!'], { type: 'text/plain' });
if (blob1.size !== 13) throw new Error(`Size test failed: expected 13, got ${blob1.size}`);
if (blob1.type !== 'text/plain') throw new Error(`Type test failed: expected 'text/plain', got '${blob1.type}'`);
if (blob1.text() !== 'Hello, World!') throw new Error(`Text test failed: expected 'Hello, World!', got '${blob1.text()}'`);

// Test 2: Blob slicing
const slice1 = blob1.slice(0, 5);
if (slice1.text() !== 'Hello') throw new Error(`Slice test failed: expected 'Hello', got '${slice1.text()}'`);

const slice2 = blob1.slice(-6);
if (slice2.text() !== 'World!') throw new Error(`Negative slice test failed: expected 'World!', got '${slice2.text()}'`);

// Test 3: File creation and properties
const file = new File(['File content here'], 'test.txt', { 
    type: 'text/plain',
    lastModified: 1640995200000
});
if (file.name !== 'test.txt') throw new Error(`File name test failed: expected 'test.txt', got '${file.name}'`);
if (file.size !== 17) throw new Error(`File size test failed: expected 17, got ${file.size}`);
if (file.type !== 'text/plain') throw new Error(`File type test failed: expected 'text/plain', got '${file.type}'`);
if (file.lastModified !== 1640995200000) throw new Error(`File lastModified test failed: expected 1640995200000, got ${file.lastModified}`);
if (file.text() !== 'File content here') throw new Error(`File text test failed: expected 'File content here', got '${file.text()}'`);

// Test 4: File inheritance - File should inherit Blob methods
const fileSlice = file.slice(5, 12);
if (fileSlice.text() !== 'content') throw new Error(`File slice test failed: expected 'content', got '${fileSlice.text()}'`);

// Test 5: Empty handling
const emptyBlob = new Blob();
if (emptyBlob.size !== 0) throw new Error(`Empty blob size test failed: expected 0, got ${emptyBlob.size}`);
if (emptyBlob.type !== '') throw new Error(`Empty blob type test failed: expected '', got '${emptyBlob.type}'`);
if (emptyBlob.text() !== '') throw new Error(`Empty blob text test failed: expected '', got '${emptyBlob.text()}'`);

// Test 6: ArrayBuffer and Bytes methods
const buffer = blob1.arrayBuffer();
if (!(buffer instanceof ArrayBuffer)) throw new Error('ArrayBuffer method failed - not an ArrayBuffer instance');

const bytes = blob1.bytes();
if (!(bytes instanceof Uint8Array)) throw new Error('Bytes method failed - not a Uint8Array instance');
if (bytes.length !== 13) throw new Error(`Bytes length test failed: expected 13, got ${bytes.length}`);

// Test 7: Error handling - File constructor should require 2 arguments
try {
    new File(['content']);
    throw new Error('File constructor should have thrown an error with missing arguments');
} catch (e) {
    if (!(e instanceof TypeError)) throw new Error('File constructor should throw TypeError for missing arguments');
}

// Return success indicator
"All integration tests passed successfully";
"#;
            
            let result: Value = cx.eval(test_script)?;
            let success_message = val_to_string(&cx, result)?;
            assert_eq!(success_message, "All integration tests passed successfully");
            
            Ok::<_, Error>(())
        })?;
        Ok(())
    }
} 