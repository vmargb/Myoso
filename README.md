# Myoso

A Kotlin + Jetpack Compose Android app with Room for SQLite storage.

## Project Structure

The project is organized into the following packages:
- `com.vmargb.myoso.model` - Data models and entities
- `com.vmargb.myoso.data` - Room database, DAOs, and repositories
- `com.vmargb.myoso.ui` - Jetpack Compose UI components and screens
- `com.vmargb.myoso.scheduling` - Scheduling-related functionality
- `com.vmargb.myoso.notes` - Notes management features
- `com.vmargb.myoso.session` - Session management features

## How to Run

### Prerequisites
- Android Studio Arctic Fox or later
- Android SDK 29 or higher
- Java 11 or higher

### Build and Run Commands

To build the project:
```bash
./gradlew assembleDebug
```

To run the app on a connected device or emulator:
```bash
./gradlew installDebug
```

To clean and rebuild:
```bash
./gradlew clean assembleDebug
```

### Development Setup
1. Open the project in Android Studio
2. Sync the project with Gradle files
3. Connect an Android device or start an emulator
4. Click the "Run" button or use the Gradle commands above

## Dependencies

- **Jetpack Compose**: Latest stable version (2024.09.00)
- **Room**: 2.6.1 for SQLite database management
- **Navigation Compose**: For navigation between screens
- **Material3**: For modern Material Design components

## Features

- Modern Jetpack Compose UI
- Room database integration
- Navigation with NavHost
- Material3 theming
- Organized package structure for scalability
