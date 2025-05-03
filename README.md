# kapy-lite

`kapy-lite` is a utility tool designed to solve specific workflow issues with Hasselblad X2d cameras and enhance photo organization for photography enthusiasts.

## Overview

As a Hasselblad X2d user, I encountered several limitations that affected my photography workflow:

- All files are stored in a single directory path rather than being organized by capture date
- The camera does not record GPS location data, which is crucial for hobby photographers to document where photos were taken

`kapy-lite` addresses these challenges with the following features:

- **Date-based Organization**: Copies photos to a target directory organized by date based on EXIF information
- **GPS Tagging**: Retrieves geo-tagging information from Google Drive (recorded with a separate mobile app) and embeds matching location data into photo EXIF metadata
- **Efficient Processing**: Supports lossless, fast copying of JPEG and HEIC files by modifying only the EXIF portion without fully decoding the entire file

## Build
### Build on macOS

If you use Homebrew (https://brew.sh/), you can easily install the required packages. <br/>
After installing Homebrew, you can install the required packages and build the application by running the following command:

```shell
$ brew install pkg-config exiv2
$ DEFAULT_CLIENT_ID={YOUR_GOOGLE_API_ID} DEFAULT_CLIENT_SECRET={YOUR_GOOGLE_API_SECRET} cargo build
```

If you are not using Homebrew, please install the required packages below and set the corresponding environment variables accordingly:

* Exiv2 library (https://exiv2.org/download.html)
  * EXIV2_INCLUDE_DIRS - list of include directories split by :
  * EXIV2_LIB_DIRS - list of lib directories split by :


### Build on Windows
#### Pre-requirements
* Exiv2 library (https://exiv2.org/download.html)
  * Provides pre-built binaries as .zip compressed files.
  * You need to set the following Windows environment variables:
    * EXIV2_INCLUDE_DIRS={YOUR_EXIV2_INCLUDE_DIR}
    * EXIV2_LIB_DIRS={YOUR_EXIV_LIB_DIR}
* clang library (https://releases.llvm.org/download.html)
  * Provides pre-built binary installers.
  * You need to set the following Windows environment variable:
    * LIBCLANG_PATH={YOUR_LLVM_BIN_DIR}

### Build
```shell
> set EXIV2_INCLUDE_DIRS={YOUR_EXIV2_INCLUDE_DIR}
> set EXIV2_LIB_DIRS={YOUR_EXIV_LIB_DIR}
> set LIBCLANG_PATH={YOUR_LLVM_BIN_DIR}
> set DEFAULT_CLIENT_ID={YOUR_GOOGLE_API_ID}
> set DEFAULT_CLIENT_SECRET={YOUR_GOOGLE_API_SECRET}
> cargo build
```


## Usage
```shell
$ kapylite clone -c ~/.kapylite.yaml --from /Volumes/Untitled/DCIM/108HASBL --to ~/images
```

## Disclaimer
To access Google Drive API using your own Google OAuth 2.0 client_id and client_secret, you will need to set up a project on the Google Developers Console and create OAuth 2.0 credentials.
Once you have obtained your credentials, you can set the CLIENT_ID and CLIENT_SECRET as environment variables or include them directly in your code.

```shell
$ CLIENT_ID={YOUR_CLIENT_ID} CLIENT_SECRET={YOUR_SECRET} kapy login
$ kapy clone
```

If you encounter login issues, you can log in again as follows.

```shell
$ kapy clean
$ CLIENT_ID={YOUR_CLIENT_ID} CLIENT_SECRET={YOUR_SECRET} kapy login
```

Or, you can assign CLIENT_ID and CLIENT_SECRET values at compile time.

```shell
$ CLIENT_ID={YOUR_CLIENT_ID} CLIENT_SECRET={YOUR_SECRET} cargo install kapy

OR

$ CLIENT_ID={YOUR_CLIENT_ID} CLIENT_SECRET={YOUR_SECRET} cargo build
```

The Google Drive API has a strict application approval process since it can access users' sensitive information.
This application was originally created for my personal use, and it is difficult to comply with Google's strict approval process.
You should refer to the following document to generate your own Google OAuth 2.0 credentials:

https://developers.google.com/identity/protocols/oauth2/native-app

The following API scopes must be specified:

* https://www.googleapis.com/auth/drive.metadata.readonly: See information about your Google Drive files.
* https://www.googleapis.com/auth/drive.readonly: See and download all your Google Drive files.


### Configurations
* An example
```yaml
default_path:
  from: /Volumes/Untitled/DCIM/108HASBL
  to: ~/images
```
