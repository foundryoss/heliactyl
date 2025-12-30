# lightd Architecture

## Project Structure

```
lightd/
├── src/
│   ├── main.rs              # Application entry point and server setup
│   ├── lib.rs               # Library exports
│   ├── config.rs            # Configuration management
│   ├── models.rs            # Data models and API types
│   ├── docker/              # Docker integration layer
│   │   ├── mod.rs           # Module exports
│   │   ├── client.rs        # Docker client connection management
│   │   ├── container.rs     # Container operations
│   │   └── volume.rs        # Volume operations
│   └── handlers/            # HTTP request handlers
│       ├── mod.rs           # Handler exports
│       ├── container.rs     # Container API endpoints
│       └── volume.rs        # Volume API endpoints
├── examples/                # Example scripts and usage
├── config.json             # Default configuration
├── Cargo.toml              # Rust dependencies
└── README.md               # Project documentation
```

## Module Organization

### Core Modules

- **main.rs**: Application bootstrap, server configuration, and routing setup
- **config.rs**: Configuration loading and management from JSON files
- **models.rs**: Shared data structures, request/response types, and API models

### Docker Layer (`src/docker/`)

The Docker layer provides a clean abstraction over the Bollard Docker client:

- **client.rs**: Docker daemon connection management and health checking
- **container.rs**: All container lifecycle operations (create, start, stop, kill, etc.)
- **volume.rs**: Volume management operations (create, list, remove)

### HTTP Layer (`src/handlers/`)

HTTP handlers are organized by resource type:

- **container.rs**: Container API endpoints with proper error handling
- **volume.rs**: Volume API endpoints with logging and validation

## Design Principles

1. **Separation of Concerns**: Each module has a single responsibility
2. **Error Handling**: Comprehensive error handling with proper logging
3. **Type Safety**: Strong typing with Rust's type system
4. **Async/Await**: Full async support for high performance
5. **Modularity**: Easy to extend with new features or Docker operations

## Adding New Features

### Adding a New Docker Operation

1. Add the operation to the appropriate manager in `src/docker/`
2. Create corresponding handler in `src/handlers/`
3. Add route in `main.rs`
4. Update models if needed

### Adding a New Resource Type

1. Create new manager in `src/docker/`
2. Create new handler module in `src/handlers/`
3. Add models to `src/models.rs`
4. Register routes in `main.rs`

## Error Handling Strategy

- Docker operations return `anyhow::Result<T>` for flexible error handling
- HTTP handlers convert errors to JSON responses with proper status codes
- All operations are logged with appropriate levels (info, warn, error)

## Configuration Management

Configuration is loaded from `config.json` and can be customized for different environments. The config structure is strongly typed and validated at startup.