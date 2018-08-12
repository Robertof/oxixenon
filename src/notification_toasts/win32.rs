//! Support for notification toasts on win32 for events.
extern crate winrt;

use std::{result, env, path, ffi};
use super::Error as NotificationError;
use winrt::*;
use winrt::windows::data::xml::dom::*;
use winrt::windows::ui::notifications::*;

const ICON_FILE_NAME: &str = "oxixenon.png";
const SHORTCUT_NAME: &str = "Xenon.lnk";
const APP_USER_MODEL_ID: &str = "RobertoFrenna.Xenon";

pub struct NotificationToasts(Option<RuntimeContext>);

impl NotificationToasts {
    pub fn new() -> NotificationToasts {
        // Check if the shortcut to make toast notifications work has been installed or not.
        let app_data = match env::var ("APPDATA") {
            Ok(val) => val,
            Err(e) => panic!("Can't retrieve APPDATA: {}", e)
        };
        let mut path = path::PathBuf::from(app_data);
        path.push (r"Microsoft\Windows\Start Menu\Programs");
        path.push (SHORTCUT_NAME);
        if !path.exists() {
            warn!("notification toasts are not configured properly");
            warn!(
                "the shortcut '{}' is required and must have AppUserModelId = '{}'",
                path.to_string_lossy(),
                APP_USER_MODEL_ID
            );
            warn!("Please read https://git.io/fNyEC for further information.");
        }
        NotificationToasts(Some(RuntimeContext::init()))
    }

    pub fn send_toast (&self, message: &str) -> result::Result<(), NotificationError> {
        if let Err(err) = self.send_toast_impl (message) {
            return Err(NotificationError(format!("WinRT/WinAPI error: {:?}", err)))
        }
        Ok(())
    }

    fn find_icon_path() -> Option<String> {
        // Try to find an icon where the binary is located.
        let mut bin_path = env::current_exe().ok()?;
        bin_path.pop(); // strip executable name
        bin_path.push (ICON_FILE_NAME);
        if bin_path.exists() {
            return Some(bin_path.to_string_lossy().into());
        }
        // Otherwise if we can't find the icon where the executable is, try to determine if
        // we're being run from cargo and if so, check in the project directory.
        bin_path.pop();
        let target_path_component = path::Component::Normal(ffi::OsStr::new("target"));
        let mut components_iter = bin_path.components();
        // Try to find a folder named "target" (where cargo executables are run) and obtain
        // the parent directory.
        if let Some(_) = components_iter.by_ref().rev().find (|v| *v == target_path_component) {
            let mut candidate_path = path::PathBuf::from (components_iter.as_path());
            candidate_path.push (ICON_FILE_NAME);
            if candidate_path.exists() {
                return Some(candidate_path.to_string_lossy().into());
            }
        }
        None
    }

    fn send_toast_impl (&self, message: &str) -> Result<()> {
        macro_rules! wrap_optional {
            // NOTE: this probably isn't the smartest error to use in this case but there
            // isn't something better.
            ($expr:expr) => ($expr.ok_or (Error::UnexpectedFailure)?)
        }
        macro_rules! wrap_optional_result {
            ($expr:expr) => (
                wrap_optional!($expr?)
            )
        }
        // Creates a text node and casts it to IXmlNode.
        macro_rules! text_node {
            ($xml:ident, $content:expr) => (
                &*wrap_optional!(
                    wrap_optional_result!(
                        $xml.create_text_node (&FastHString::new ($content))
                    ).query_interface::<IXmlNode>()
                )
            )
        }
        let is_message_multiline = message.contains ("\n");
        let toast_xml = match Self::find_icon_path() {
            Some(icon_path) => {
                // Use "ToastImageAndText02" as the base toast template if we got an icon.
                let xml = wrap_optional_result!(
                    ToastNotificationManager::get_template_content (
                        if is_message_multiline { ToastTemplateType::ToastImageAndText02 }
                        else                    { ToastTemplateType::ToastImageAndText01 }
                    )
                );
                let toast_img_tag = {
                    // First, retrieve a collection of <image> tags. (there's only one)
                    let img_tags = wrap_optional_result!(
                        xml.get_elements_by_tag_name (&FastHString::new ("image"))
                    );
                    // Then, retrieve the first one and convert it to an XmlElement.
                    wrap_optional!(
                        wrap_optional_result!(img_tags.item(0)).query_interface::<XmlElement>()
                    )
                };
                // Set the source of the image to our icon file.
                toast_img_tag.set_attribute (
                    &FastHString::new ("src"),
                    &FastHString::new (
                        // File paths have to be specified as file:///C:/Users/...
                        format!("file:///{}", icon_path.replace (r"\", "/")).as_str()
                    )
                )?;
                xml
            },
            // Use a textual template otherwise.
            None => wrap_optional_result!(ToastNotificationManager::get_template_content (
                if is_message_multiline { ToastTemplateType::ToastText02 }
                else                    { ToastTemplateType::ToastText01 }
            ))
        };
        // Now, set the text.
        let toast_text_tags = wrap_optional_result!(
            toast_xml.get_elements_by_tag_name (&FastHString::new ("text"))
        );
        let mut message_lines = message.lines();
        wrap_optional_result!(toast_text_tags.item(0)).append_child (
            text_node!(toast_xml, message_lines.next().unwrap())
        )?;
        if is_message_multiline {
            wrap_optional_result!(toast_text_tags.item(1)).append_child (
                text_node!(toast_xml, message_lines.next().unwrap())
            )?;
        }
        // Finally, we're ready to create and show the toast.
        let toast = ToastNotification::create_toast_notification (&*toast_xml)?;
        wrap_optional_result!(
            ToastNotificationManager::create_toast_notifier_with_id (
                &FastHString::new (APP_USER_MODEL_ID)
            )
        ).show (&*toast)
    }
}

impl Drop for NotificationToasts {
    fn drop(&mut self) {
        // Be sure to cleanup our RuntimeContext if we're being dropped.
        if let Some(context) = self.0.take() {
            context.uninit()
        }
    }
}
