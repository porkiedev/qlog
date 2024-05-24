//
// This file contains functions that convert to/from maidenhead locators (Grid Squares) and longitude/latitude
//

use geoutils::Location;

/// Converts a Latitude and Longitude to a 6-character grid square (e.g. "DM79mr");
pub fn lat_lon_to_grid(location: &Location) -> String {

    // Allocate a string with 6 characters (4 bytes each)
    let mut grid = String::with_capacity(4*6);

    // Get the lon and lat of the input location and add an offset to keep the value positive
    let mut lon = location.longitude() + 180.0;
    let mut lat = location.latitude() + 90.0;

    // 1st character; Longitude with 20 degrees of precision
    let c1 = (lon / 20.0) as u8;
    grid.push((c1  + 65) as char);

    // 2nd character; Latitude with 10 degrees of precision
    let c2 = (lat / 10.0) as u8;
    grid.push((c2  + 65) as char);

    // 3rd character; Longitude with 2 degrees of precision
    let c3 = ((lon - (c1 as f64 * 20.0)) / 2.0) as u8;
    grid.push((c3 + 48) as char);

    // 4th character; Latitude with 1 degree of precision
    let c4 = ((lat - (c2 as f64 * 10.0)) / 1.0) as u8;
    grid.push((c4 + 48) as char);

    // 5th character; Longitude with 1/24th of a degree of precision
    let c5 = ((lon - (c1 as f64 * 20.0)) % 1.0 * 12.0) as u8;
    grid.push((c5 + 97) as char);
        
    // 6th character; Latitude with 1/12th of a degree of precision
    let c6 = ((lat - (c2 as f64 * 10.0)) % 1.0 * 24.0) as u8;
    grid.push((c6 + 97) as char);

    grid
}

/// Converts a 6-character grid square into a Latitude and Longitude
/// 
/// WARNING: For performance reasons, this function does not *currently* provide input validation. In other words,
/// giving this function a string with an invalid grid square (random characters, characters that are out of range, etc) will provide an unusual output,
/// and possibly cause a panic.
/// 
/// NOTE: This function only supports up to 6 characters. Anything more will provide invalid results.
pub fn grid_to_lat_lon(grid: &str) -> Location {

    // Create the latitude and longitude values
    let mut lat = 0.0;
    let mut lon = 0.0;

    // Used to efficiently count the number of characters in the string
    let mut length = 0u8;

    // Iterate through the characters
    for (idx, mut character) in grid.char_indices() {
        // Convert character to uppercase
        character = character.to_ascii_uppercase();

        // Increment the total character count
        length += 1;

        let num = if character.is_ascii_digit() {
            // Convert the character into its decimal value and subtract 48 to apply an offset. This gives us the number that the digit represents.
            character as u32 - 48
        } else {
            // Convert the unicode character into its decimal value and subtract 65 to apply an offset. This gives us the alphabet index for each character.
            character as u32 - 65
        };

        // 1st character; Longitude with 20 degrees of precision
        if idx == 0 {
            lon += num as f64 * 20.0;
        }
        // 2nd character; Latitude with 10 degrees of precision
        else if idx == 1 {
            lat += num as f64 * 10.0;
        }
        // 3rd character; Longitude with 2 degrees of precision
        else if idx == 2 {
            lon += num as f64 * 2.0;
        }
        // 4th character; Latitude with 1 degree of precision
        else if idx == 3 {
            lat += num as f64;
        }
        // 5th character; Longitude with 2/24th of a degree of precision
        else if idx == 4 {
            lon += num as f64 * (2.0 / 24.0);
        }
        // 6th character; Latitude with 1/24th of a degree of precision
        else if idx == 5 {
            lat += num as f64 * (1.0 / 24.0);
        }

    }

    // Apply an offset to the location so we're centered in the middle of the grid square
    // The offset value varies depending on how many characters are in our grid square (i.e. the precision)
    if length == 2 {
        lat += 5.0;
        lon += 10.0;
    }
    else if length == 4 {
        lat += 0.5;
        lon += 1.0;
    }
    else if length == 6 {
        lat += (1.0 / 24.0) * 0.5;
        lon += (2.0 / 24.0) * 0.5;
    }

    // Subtract 90.0 and 180.0 degrees from the latitude and longitude to make them normal again
    lat -= 90.0;
    lon -= 180.0;
    
    Location::new(lat, lon)

}
