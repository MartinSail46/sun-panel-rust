declare namespace SystemMonitor {

    interface CPUInfo {
        coreCount: number
        cpuNum: number
        model: string
        usages: number[]
    }

    interface DiskInfo {
        mountpoint: string
        total: number
        used: number
        free: number
        usedPercent: number
    }

    interface MemoryInfo {
        available?: number
        total: number
        used: number
        free: number
        usedPercent: number
    }

    interface Mountpoint{
        device:string
        mountpoint:string
        fstype:string
    }
}