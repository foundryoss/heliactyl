package filesystem

import (
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"github.com/sirupsen/logrus"

	"NightLightd/internal/config"
)

// FileInfo represents detailed file information
type FileInfo struct {
	Name        string    `json:"name"`
	IsDirectory bool      `json:"isDirectory"`
	IsEditable  bool      `json:"isEditable"`
	Size        string    `json:"size"`
	LastUpdated time.Time `json:"lastUpdated"`
	Purpose     string    `json:"purpose"`
	Extension   string    `json:"extension"`
	Permissions string    `json:"permissions"`
}

// Manager handles file system operations
type Manager struct {
	config      *config.Config
	log         *logrus.Logger
	volumesPath string
}

// NewManager creates a new filesystem manager
func NewManager(cfg *config.Config, log *logrus.Logger) *Manager {
	volumesPath := cfg.VolumesPath
	// Ensure path is absolute
	if !filepath.IsAbs(volumesPath) {
		if cwd, err := os.Getwd(); err == nil {
			volumesPath = filepath.Join(cwd, volumesPath)
		}
	}

	return &Manager{
		config:      cfg,
		log:         log,
		volumesPath: volumesPath,
	}
}

// validateVolumeExists checks if a volume directory exists and is properly configured
func (m *Manager) validateVolumeExists(volumeID string) error {
	volumePath := filepath.Join(m.volumesPath, volumeID)

	// Check if volume directory exists
	if _, err := os.Stat(volumePath); os.IsNotExist(err) {
		return fmt.Errorf("volume directory does not exist: %s", volumeID)
	}

	// Ensure it's actually a directory
	if info, err := os.Stat(volumePath); err != nil {
		return fmt.Errorf("cannot access volume directory: %w", err)
	} else if !info.IsDir() {
		return fmt.Errorf("volume path is not a directory: %s", volumeID)
	}

	return nil
}

// ListFiles lists files in a volume directory
func (m *Manager) ListFiles(volumeID, subPath string) ([]FileInfo, error) {
	// Validate volume exists
	if err := m.validateVolumeExists(volumeID); err != nil {
		return nil, err
	}

	volumePath := filepath.Join(m.volumesPath, volumeID)
	targetPath, err := m.safePath(volumePath, subPath)
	if err != nil {
		return nil, err
	}

	entries, err := os.ReadDir(targetPath)
	if err != nil {
		return nil, fmt.Errorf("failed to read directory: %w", err)
	}

	var files []FileInfo
	for _, entry := range entries {
		info, err := entry.Info()
		if err != nil {
			m.log.WithError(err).Warn("Failed to get file info")
			continue
		}

		fileInfo := FileInfo{
			Name:        entry.Name(),
			IsDirectory: entry.IsDir(),
			IsEditable:  m.isEditable(entry.Name()),
			LastUpdated: info.ModTime(),
			Extension:   strings.ToLower(filepath.Ext(entry.Name())),
			Permissions: fmt.Sprintf("%o", info.Mode().Perm()),
		}

		if entry.IsDir() {
			dirSize, err := m.calculateDirectorySize(filepath.Join(targetPath, entry.Name()))
			if err != nil {
				fileInfo.Size = "Unknown"
			} else {
				fileInfo.Size = m.formatFileSize(dirSize)
			}
			fileInfo.Purpose = "folder"
		} else {
			fileInfo.Size = m.formatFileSize(info.Size())
			fileInfo.Purpose = m.getFilePurpose(entry.Name())
		}

		files = append(files, fileInfo)
	}

	return files, nil
}

// ReadFile reads the contents of a file
func (m *Manager) ReadFile(volumeID, filePath string) ([]byte, error) {
	// Validate volume exists
	if err := m.validateVolumeExists(volumeID); err != nil {
		return nil, err
	}

	volumePath := filepath.Join(m.volumesPath, volumeID)
	targetPath, err := m.safePath(volumePath, filePath)
	if err != nil {
		return nil, err
	}

	if !m.isEditable(filepath.Base(targetPath)) {
		return nil, fmt.Errorf("file type not supported for viewing")
	}

	data, err := os.ReadFile(targetPath)
	if err != nil {
		return nil, fmt.Errorf("failed to read file: %w", err)
	}

	return data, nil
}

// WriteFile writes content to a file
func (m *Manager) WriteFile(volumeID, filePath string, content []byte) error {
	return m.writeFileInternal(volumeID, filePath, content, true)
}

// CreateFile creates a new file with content
func (m *Manager) CreateFile(volumeID, filePath string, content []byte) error {
	return m.writeFileInternal(volumeID, filePath, content, false)
}

// writeFileInternal handles both file creation and editing
func (m *Manager) writeFileInternal(volumeID, filePath string, content []byte, checkEditable bool) error {
	// Validate volume exists
	if err := m.validateVolumeExists(volumeID); err != nil {
		return err
	}

	volumePath := filepath.Join(m.volumesPath, volumeID)
	targetPath, err := m.safePath(volumePath, filePath)
	if err != nil {
		return err
	}

	// Check if file is editable (for editing operations)
	if checkEditable && !m.isEditable(filepath.Base(targetPath)) {
		return fmt.Errorf("file type not supported for editing")
	}

	// Create directory if it doesn't exist
	dir := filepath.Dir(targetPath)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return fmt.Errorf("failed to create directory: %w", err)
	}

	if err := os.WriteFile(targetPath, content, 0644); err != nil {
		return fmt.Errorf("failed to write file: %w", err)
	}

	m.log.WithFields(logrus.Fields{
		"volumeId": volumeID,
		"file":     filePath,
		"size":     len(content),
	}).Debug("File written successfully")

	return nil
}

// DeleteFile deletes a file or directory
func (m *Manager) DeleteFile(volumeID, filePath string) error {
	volumePath := filepath.Join(m.volumesPath, volumeID)
	targetPath, err := m.safePath(volumePath, filePath)
	if err != nil {
		return err
	}

	if err := os.RemoveAll(targetPath); err != nil {
		return fmt.Errorf("failed to delete file: %w", err)
	}

	m.log.WithFields(logrus.Fields{
		"volumeId": volumeID,
		"file":     filePath,
	}).Debug("File deleted successfully")

	return nil
}

// RenameFile renames a file or directory
func (m *Manager) RenameFile(volumeID, oldPath, newName string) error {
	volumePath := filepath.Join(m.volumesPath, volumeID)
	oldTargetPath, err := m.safePath(volumePath, oldPath)
	if err != nil {
		return err
	}

	newTargetPath := filepath.Join(filepath.Dir(oldTargetPath), newName)

	// Ensure new path is also safe
	if _, err := m.safePath(volumePath, filepath.Join(filepath.Dir(oldPath), newName)); err != nil {
		return err
	}

	if err := os.Rename(oldTargetPath, newTargetPath); err != nil {
		return fmt.Errorf("failed to rename file: %w", err)
	}

	m.log.WithFields(logrus.Fields{
		"volumeId": volumeID,
		"oldPath":  oldPath,
		"newName":  newName,
	}).Debug("File renamed successfully")

	return nil
}

// CreateDirectory creates a new directory
func (m *Manager) CreateDirectory(volumeID, dirPath string) error {
	volumePath := filepath.Join(m.volumesPath, volumeID)
	targetPath, err := m.safePath(volumePath, dirPath)
	if err != nil {
		return err
	}

	if err := os.MkdirAll(targetPath, 0755); err != nil {
		return fmt.Errorf("failed to create directory: %w", err)
	}

	m.log.WithFields(logrus.Fields{
		"volumeId": volumeID,
		"path":     dirPath,
	}).Debug("Directory created successfully")

	return nil
}

// SearchFiles searches for files matching a query
func (m *Manager) SearchFiles(volumeID, query string) ([]FileInfo, error) {
	volumePath := filepath.Join(m.volumesPath, volumeID)

	var results []FileInfo
	err := filepath.Walk(volumePath, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return nil // Skip errors and continue
		}

		// Get relative path from volume root
		relPath, err := filepath.Rel(volumePath, path)
		if err != nil {
			return nil
		}

		// Skip if not matching query
		if !strings.Contains(strings.ToLower(info.Name()), strings.ToLower(query)) {
			return nil
		}

		fileInfo := FileInfo{
			Name:        relPath,
			IsDirectory: info.IsDir(),
			Size:        m.formatFileSize(info.Size()),
			LastUpdated: info.ModTime(),
			Purpose:     m.getFilePurpose(info.Name()),
		}

		results = append(results, fileInfo)
		return nil
	})

	if err != nil {
		return nil, fmt.Errorf("failed to search files: %w", err)
	}

	return results, nil
}

// StreamFile returns a reader for streaming file content
func (m *Manager) StreamFile(volumeID, filePath string) (io.ReadCloser, error) {
	volumePath := filepath.Join(m.volumesPath, volumeID)
	targetPath, err := m.safePath(volumePath, filePath)
	if err != nil {
		return nil, err
	}

	file, err := os.Open(targetPath)
	if err != nil {
		return nil, fmt.Errorf("failed to open file: %w", err)
	}

	return file, nil
}

// safePath ensures the path is within the volume directory and adds security validation
func (m *Manager) safePath(volumePath, subPath string) (string, error) {
	// Ensure volumePath is within the configured volumes directory
	if !strings.HasPrefix(volumePath, m.volumesPath) {
		return "", fmt.Errorf("volume path is outside configured volumes directory")
	}

	// Handle root paths
	if subPath == "" || subPath == "/" {
		return volumePath, nil
	}

	// Security check: reject dangerous characters and patterns
	if strings.Contains(subPath, "..") ||
		strings.Contains(subPath, "~") ||
		strings.HasPrefix(subPath, "/etc") ||
		strings.HasPrefix(subPath, "/proc") ||
		strings.HasPrefix(subPath, "/sys") ||
		strings.HasPrefix(subPath, "/dev") ||
		strings.HasPrefix(subPath, "/root") ||
		strings.HasPrefix(subPath, "/home") {
		return "", fmt.Errorf("path contains forbidden patterns")
	}

	// Clean the path to prevent directory traversal
	cleanPath := filepath.Clean(subPath)

	// If after cleaning, the path is "." or "/", return volume root
	if cleanPath == "." || cleanPath == "/" {
		return volumePath, nil
	}

	// Additional check after cleaning
	if strings.Contains(cleanPath, "..") {
		return "", fmt.Errorf("path traversal detected after cleaning")
	}

	// Join with volume path
	fullPath := filepath.Join(volumePath, cleanPath)

	// CRITICAL: Ensure the result is still within the volume directory
	if !strings.HasPrefix(fullPath, volumePath+string(filepath.Separator)) && fullPath != volumePath {
		return "", fmt.Errorf("attempting to access outside of the volume directory")
	}

	// Final check: ensure we're still within the volumes root
	if !strings.HasPrefix(fullPath, m.volumesPath) {
		return "", fmt.Errorf("final path is outside volumes directory")
	}

	return fullPath, nil
}

// isEditable determines if a file can be edited based on its extension
func (m *Manager) isEditable(filename string) bool {
	ext := strings.ToLower(filepath.Ext(filename))
	editableExtensions := map[string]bool{
		// Text files
		".txt": true,
		".md":  true,
		".rtf": true,
		".log": true,
		".ini": true,
		".csv": true,

		// Web development
		".html": true,
		".htm":  true,
		".css":  true,
		".scss": true,
		".sass": true,
		".less": true,
		".js":   true,
		".ts":   true,
		".jsx":  true,
		".tsx":  true,
		".json": true,
		".xml":  true,
		".svg":  true,

		// Programming languages
		".py":     true,
		".java":   true,
		".c":      true,
		".cpp":    true,
		".h":      true,
		".hpp":    true,
		".cs":     true,
		".go":     true,
		".rb":     true,
		".php":    true,
		".swift":  true,
		".kt":     true,
		".rs":     true,
		".scala":  true,
		".groovy": true,

		// Scripting
		".sh":   true,
		".bash": true,
		".ps1":  true,
		".bat":  true,
		".cmd":  true,

		// Markup and config
		".yaml":       true,
		".yml":        true,
		".toml":       true,
		".cfg":        true,
		".conf":       true,
		".properties": true,

		// Document formats
		".tex":      true,
		".bib":      true,
		".markdown": true,

		// Database
		".sql": true,

		// Files without extension or special files
		"": true,
	}

	// Check for special filenames
	name := strings.ToLower(filename)
	specialFiles := []string{
		".gitignore",
		".env",
		".htaccess",
		"dockerfile",
		"makefile",
		"readme",
	}

	for _, special := range specialFiles {
		if strings.Contains(name, special) {
			return true
		}
	}

	return editableExtensions[ext]
}

// getFilePurpose determines the purpose of a file based on its name/extension
func (m *Manager) getFilePurpose(filename string) string {
	ext := strings.ToLower(filepath.Ext(filename))
	name := strings.ToLower(filename)

	switch {
	case ext == ".log":
		return "log"
	case ext == ".json" || ext == ".yml" || ext == ".yaml":
		return "config"
	case ext == ".jar":
		return "executable"
	case ext == ".zip" || ext == ".tar" || ext == ".gz":
		return "archive"
	case strings.Contains(name, "readme"):
		return "documentation"
	case ext == ".md":
		return "documentation"
	case ext == ".txt":
		return "text"
	default:
		return "file"
	}
}

// formatFileSize formats file size in human-readable format
func (m *Manager) formatFileSize(size int64) string {
	if size == 0 {
		return "0 B"
	}

	units := []string{"B", "KB", "MB", "GB", "TB"}
	unitIndex := 0
	sizeFloat := float64(size)

	for sizeFloat >= 1024 && unitIndex < len(units)-1 {
		sizeFloat /= 1024
		unitIndex++
	}

	if unitIndex == 0 {
		return fmt.Sprintf("%.0f %s", sizeFloat, units[unitIndex])
	}
	return fmt.Sprintf("%.2f %s", sizeFloat, units[unitIndex])
}

// calculateDirectorySize calculates the total size of a directory
func (m *Manager) calculateDirectorySize(dirPath string) (int64, error) {
	var totalSize int64

	err := filepath.Walk(dirPath, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return nil // Skip errors and continue
		}
		if !info.IsDir() {
			totalSize += info.Size()
		}
		return nil
	})

	return totalSize, err
}

// GetFilePermissions gets the permissions of a file
func (m *Manager) GetFilePermissions(volumeID, filePath string) (string, error) {
	volumePath := filepath.Join(m.volumesPath, volumeID)
	targetPath, err := m.safePath(volumePath, filePath)
	if err != nil {
		return "", err
	}

	info, err := os.Stat(targetPath)
	if err != nil {
		return "", fmt.Errorf("failed to get file info: %w", err)
	}

	return info.Mode().String(), nil
}

// SetFilePermissions sets the permissions of a file
func (m *Manager) SetFilePermissions(volumeID, filePath, mode string) error {
	volumePath := filepath.Join(m.volumesPath, volumeID)
	targetPath, err := m.safePath(volumePath, filePath)
	if err != nil {
		return err
	}

	// Parse mode string (e.g., "0644", "755")
	perm, err := strconv.ParseUint(mode, 8, 32)
	if err != nil {
		return fmt.Errorf("invalid permission mode: %s", mode)
	}

	if err := os.Chmod(targetPath, os.FileMode(perm)); err != nil {
		return fmt.Errorf("failed to change permissions: %w", err)
	}

	m.log.WithFields(logrus.Fields{
		"volumeId": volumeID,
		"file":     filePath,
		"mode":     mode,
	}).Info("File permissions changed")

	return nil
}

// StreamFileContent streams file content to an HTTP response writer (for live streaming)
func (m *Manager) StreamFileContent(w http.ResponseWriter, volumeID, filePath string) error {
	volumePath := filepath.Join(m.volumesPath, volumeID)
	targetPath, err := m.safePath(volumePath, filePath)
	if err != nil {
		return err
	}

	file, err := os.Open(targetPath)
	if err != nil {
		return fmt.Errorf("failed to open file: %w", err)
	}
	defer file.Close()

	// Get file info
	info, err := file.Stat()
	if err != nil {
		return fmt.Errorf("failed to get file info: %w", err)
	}

	// For regular files, just stream the content
	if info.Mode().IsRegular() {
		_, err := io.Copy(w, file)
		return err
	}

	// For special files (like logs), implement tailing behavior
	return m.tailFile(w, file)
}

// tailFile implements file tailing for log streaming
func (m *Manager) tailFile(w http.ResponseWriter, file *os.File) error {
	// Seek to end of file
	if _, err := file.Seek(0, io.SeekEnd); err != nil {
		return fmt.Errorf("failed to seek to end: %w", err)
	}

	// Create a ticker for periodic checks
	ticker := time.NewTicker(1 * time.Second)
	defer ticker.Stop()

	buffer := make([]byte, 4096)

	for {
		select {
		case <-ticker.C:
			// Try to read new content
			n, err := file.Read(buffer)
			if err != nil && err != io.EOF {
				return fmt.Errorf("failed to read file: %w", err)
			}

			if n > 0 {
				if _, err := w.Write(buffer[:n]); err != nil {
					return fmt.Errorf("failed to write to response: %w", err)
				}

				// Flush if possible
				if f, ok := w.(http.Flusher); ok {
					f.Flush()
				}
			}
		}
	}
}

// SearchFilesInPath searches for files in a specific path
func (m *Manager) SearchFilesInPath(volumeID, searchPath, query string) ([]FileInfo, error) {
	volumePath := filepath.Join(m.volumesPath, volumeID)
	fullPath, err := m.safePath(volumePath, searchPath)
	if err != nil {
		return nil, err
	}

	var results []FileInfo

	err = filepath.Walk(fullPath, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return nil // Skip files with errors
		}

		// Get relative path from search root
		relPath, err := filepath.Rel(fullPath, path)
		if err != nil {
			return nil
		}

		// Skip if it's the root directory
		if relPath == "." {
			return nil
		}

		// Check if filename matches query (case-insensitive)
		if strings.Contains(strings.ToLower(info.Name()), strings.ToLower(query)) {
			fileInfo := FileInfo{
				Name:        info.Name(),
				IsDirectory: info.IsDir(),
				Size:        m.formatFileSize(info.Size()),
				LastUpdated: info.ModTime(),
				Purpose:     m.getFilePurpose(info.Name()),
				Extension:   filepath.Ext(info.Name()),
				Permissions: info.Mode().String(),
			}

			results = append(results, fileInfo)
		}

		return nil
	})

	if err != nil {
		return nil, fmt.Errorf("failed to search files: %w", err)
	}

	return results, nil
}
