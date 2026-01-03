import {
    FolderIcon,
    DocumentIcon,
    DocumentTextIcon,
    PhotoIcon,
    CodeBracketIcon,
    MusicalNoteIcon,
    VideoCameraIcon,
    ArchiveBoxIcon,
    LockClosedIcon,
} from '@heroicons/react/24/outline'

export interface FileIconProps {
    fileName: string
    isDirectory: boolean
    className?: string
}

/**
 * Returns the appropriate icon component for a file based on its extension
 */
export function FileIcon({ fileName, isDirectory, className = 'h-5 w-5' }: FileIconProps) {
    if (isDirectory) {
        return <FolderIcon className={`${className} text-blue-500`} />
    }
    
    const ext = fileName.split('.').pop()?.toLowerCase()
    
    if (!ext) {
        return <DocumentIcon className={`${className} text-neutral-500`} />
    }
    
    // Programming Languages
    if (['py', 'pyc', 'pyo', 'pyd'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-blue-600`} /> // Python
    }
    if (['js', 'mjs', 'cjs'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-yellow-500`} /> // JavaScript
    }
    if (['ts', 'tsx'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-blue-500`} /> // TypeScript
    }
    if (['jsx'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-cyan-500`} /> // React
    }
    if (['java', 'class'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-red-600`} /> // Java
    }
    if (['jar', 'war', 'ear'].includes(ext)) {
        return <ArchiveBoxIcon className={`${className} text-orange-600`} /> // Java Archives
    }
    if (['go'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-cyan-600`} /> // Go
    }
    if (['rs'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-orange-700`} /> // Rust
    }
    if (['c', 'h'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-blue-700`} /> // C
    }
    if (['cpp', 'cc', 'cxx', 'hpp', 'hxx'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-pink-600`} /> // C++
    }
    if (['cs'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-purple-600`} /> // C#
    }
    if (['php'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-indigo-600`} /> // PHP
    }
    if (['rb', 'erb'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-red-500`} /> // Ruby
    }
    if (['swift'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-orange-500`} /> // Swift
    }
    if (['kt', 'kts'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-purple-500`} /> // Kotlin
    }
    if (['scala'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-red-700`} /> // Scala
    }
    if (['dart'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-blue-400`} /> // Dart
    }
    if (['lua'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-blue-500`} /> // Lua
    }
    if (['r', 'rmd'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-blue-600`} /> // R
    }
    if (['pl', 'pm'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-blue-500`} /> // Perl
    }
    
    // Shell/Scripts
    if (['sh', 'bash', 'zsh', 'fish'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-green-600`} /> // Shell
    }
    if (['bat', 'cmd', 'ps1'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-blue-400`} /> // Windows Scripts
    }
    
    // Web
    if (['html', 'htm'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-orange-600`} /> // HTML
    }
    if (['css', 'scss', 'sass', 'less'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-blue-500`} /> // CSS
    }
    if (['vue'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-green-500`} /> // Vue
    }
    if (['svelte'].includes(ext)) {
        return <CodeBracketIcon className={`${className} text-orange-500`} /> // Svelte
    }
    
    // Data/Config
    if (['json', 'jsonc'].includes(ext)) {
        return <DocumentTextIcon className={`${className} text-yellow-600`} /> // JSON
    }
    if (['xml'].includes(ext)) {
        return <DocumentTextIcon className={`${className} text-orange-500`} /> // XML
    }
    if (['yml', 'yaml'].includes(ext)) {
        return <DocumentTextIcon className={`${className} text-red-500`} /> // YAML
    }
    if (['toml'].includes(ext)) {
        return <DocumentTextIcon className={`${className} text-gray-600`} /> // TOML
    }
    if (['ini', 'cfg', 'conf', 'config'].includes(ext)) {
        return <DocumentTextIcon className={`${className} text-gray-500`} /> // Config
    }
    if (['env'].includes(ext)) {
        return <DocumentTextIcon className={`${className} text-green-600`} /> // Environment
    }
    if (['properties'].includes(ext)) {
        return <DocumentTextIcon className={`${className} text-gray-500`} /> // Properties
    }
    
    // Documents
    if (['txt', 'text'].includes(ext)) {
        return <DocumentTextIcon className={`${className} text-neutral-500`} />
    }
    if (['md', 'markdown'].includes(ext)) {
        return <DocumentTextIcon className={`${className} text-blue-600`} />
    }
    if (['log'].includes(ext)) {
        return <DocumentTextIcon className={`${className} text-gray-600`} />
    }
    if (['pdf'].includes(ext)) {
        return <DocumentIcon className={`${className} text-red-600`} />
    }
    if (['doc', 'docx'].includes(ext)) {
        return <DocumentIcon className={`${className} text-blue-600`} />
    }
    if (['xls', 'xlsx'].includes(ext)) {
        return <DocumentIcon className={`${className} text-green-600`} />
    }
    if (['ppt', 'pptx'].includes(ext)) {
        return <DocumentIcon className={`${className} text-orange-600`} />
    }
    if (['rtf'].includes(ext)) {
        return <DocumentIcon className={`${className} text-blue-500`} />
    }
    if (['csv'].includes(ext)) {
        return <DocumentIcon className={`${className} text-green-500`} />
    }
    
    // Images
    if (['jpg', 'jpeg'].includes(ext)) {
        return <PhotoIcon className={`${className} text-blue-500`} />
    }
    if (['png'].includes(ext)) {
        return <PhotoIcon className={`${className} text-purple-500`} />
    }
    if (['gif'].includes(ext)) {
        return <PhotoIcon className={`${className} text-green-500`} />
    }
    if (['svg'].includes(ext)) {
        return <PhotoIcon className={`${className} text-yellow-500`} />
    }
    if (['webp', 'ico', 'bmp', 'tiff', 'tif'].includes(ext)) {
        return <PhotoIcon className={`${className} text-purple-400`} />
    }
    if (['psd', 'ai', 'sketch'].includes(ext)) {
        return <PhotoIcon className={`${className} text-blue-600`} />
    }
    
    // Audio
    if (['mp3'].includes(ext)) {
        return <MusicalNoteIcon className={`${className} text-pink-500`} />
    }
    if (['wav', 'flac'].includes(ext)) {
        return <MusicalNoteIcon className={`${className} text-blue-500`} />
    }
    if (['ogg', 'oga', 'm4a', 'aac', 'wma'].includes(ext)) {
        return <MusicalNoteIcon className={`${className} text-yellow-500`} />
    }
    if (['midi', 'mid'].includes(ext)) {
        return <MusicalNoteIcon className={`${className} text-purple-500`} />
    }
    
    // Video
    if (['mp4', 'm4v'].includes(ext)) {
        return <VideoCameraIcon className={`${className} text-red-500`} />
    }
    if (['webm'].includes(ext)) {
        return <VideoCameraIcon className={`${className} text-green-500`} />
    }
    if (['avi', 'mkv', 'mov', 'wmv', 'flv', 'mpg', 'mpeg'].includes(ext)) {
        return <VideoCameraIcon className={`${className} text-pink-500`} />
    }
    
    // Archives
    if (['zip'].includes(ext)) {
        return <ArchiveBoxIcon className={`${className} text-yellow-600`} />
    }
    if (['tar', 'gz', 'tgz', 'tar.gz'].includes(ext) || fileName.endsWith('.tar.gz')) {
        return <ArchiveBoxIcon className={`${className} text-orange-600`} />
    }
    if (['rar'].includes(ext)) {
        return <ArchiveBoxIcon className={`${className} text-purple-600`} />
    }
    if (['7z'].includes(ext)) {
        return <ArchiveBoxIcon className={`${className} text-gray-600`} />
    }
    if (['bz2', 'xz', 'lz', 'lzma'].includes(ext)) {
        return <ArchiveBoxIcon className={`${className} text-blue-600`} />
    }
    if (['iso', 'dmg', 'img'].includes(ext)) {
        return <ArchiveBoxIcon className={`${className} text-purple-500`} />
    }
    
    // Database
    if (['sql', 'db', 'sqlite', 'sqlite3', 'mdb'].includes(ext)) {
        return <DocumentIcon className={`${className} text-blue-700`} />
    }
    
    // Fonts
    if (['ttf', 'otf', 'woff', 'woff2', 'eot'].includes(ext)) {
        return <DocumentIcon className={`${className} text-gray-600`} />
    }
    
    // Certificates/Keys
    if (['pem', 'crt', 'cer', 'key', 'pub', 'p12', 'pfx'].includes(ext)) {
        return <LockClosedIcon className={`${className} text-yellow-600`} />
    }
    if (['lock'].includes(ext)) {
        return <LockClosedIcon className={`${className} text-yellow-600`} />
    }
    
    // Build/Package files
    if (['dockerfile', 'dockerignore'].includes(fileName.toLowerCase())) {
        return <DocumentTextIcon className={`${className} text-blue-500`} />
    }
    if (['makefile', 'cmake'].includes(fileName.toLowerCase())) {
        return <DocumentTextIcon className={`${className} text-orange-500`} />
    }
    if (['package.json', 'package-lock.json', 'yarn.lock', 'pnpm-lock.yaml'].includes(fileName.toLowerCase())) {
        return <DocumentTextIcon className={`${className} text-red-500`} />
    }
    if (['cargo.toml', 'cargo.lock'].includes(fileName.toLowerCase())) {
        return <DocumentTextIcon className={`${className} text-orange-600`} />
    }
    if (['gemfile', 'gemfile.lock'].includes(fileName.toLowerCase())) {
        return <DocumentTextIcon className={`${className} text-red-500`} />
    }
    if (['requirements.txt', 'pipfile', 'poetry.lock'].includes(fileName.toLowerCase())) {
        return <DocumentTextIcon className={`${className} text-blue-500`} />
    }
    if (['go.mod', 'go.sum'].includes(fileName.toLowerCase())) {
        return <DocumentTextIcon className={`${className} text-cyan-600`} />
    }
    
    // Git files
    if (['.gitignore', '.gitattributes', '.gitmodules'].includes(fileName.toLowerCase())) {
        return <DocumentTextIcon className={`${className} text-orange-600`} />
    }
    
    // Default
    return <DocumentIcon className={`${className} text-neutral-400`} />
}

/**
 * Get file type category for a given filename
 */
export function getFileCategory(fileName: string, isDirectory: boolean): string {
    if (isDirectory) return 'directory'
    
    const ext = fileName.split('.').pop()?.toLowerCase()
    if (!ext) return 'unknown'
    
    // Programming
    if (['py', 'js', 'ts', 'tsx', 'jsx', 'java', 'go', 'rs', 'c', 'cpp', 'cs', 'php', 'rb', 'swift', 'kt', 'scala'].includes(ext)) {
        return 'code'
    }
    
    // Web
    if (['html', 'css', 'scss', 'sass', 'less', 'vue', 'svelte'].includes(ext)) {
        return 'web'
    }
    
    // Data/Config
    if (['json', 'xml', 'yml', 'yaml', 'toml', 'ini', 'cfg', 'conf', 'env'].includes(ext)) {
        return 'config'
    }
    
    // Documents
    if (['txt', 'md', 'pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx'].includes(ext)) {
        return 'document'
    }
    
    // Images
    if (['jpg', 'jpeg', 'png', 'gif', 'svg', 'webp', 'ico', 'bmp'].includes(ext)) {
        return 'image'
    }
    
    // Audio
    if (['mp3', 'wav', 'flac', 'ogg', 'm4a', 'aac'].includes(ext)) {
        return 'audio'
    }
    
    // Video
    if (['mp4', 'webm', 'avi', 'mkv', 'mov', 'wmv'].includes(ext)) {
        return 'video'
    }
    
    // Archives
    if (['zip', 'tar', 'gz', 'rar', '7z', 'bz2'].includes(ext)) {
        return 'archive'
    }
    
    return 'file'
}
