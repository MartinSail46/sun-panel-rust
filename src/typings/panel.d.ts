declare namespace Panel {

    interface ItemInfo extends Common.InfoBase {
        icon: ItemIcon |null
        title: string
        url: string
        sort?: number
        description?: string
        openMethod: number
        itemIconGroupId ?:number
    }

    interface ItemIconGroup extends Common.InfoBase {
        description?: string
        icon?: string
        title?: string
        sort?:number
    }

    interface ItemIcon {
        itemType: number
        src ?: string
        text ?: string
        backgroundColor ?: string
        style?: 0 | 1
        textColor?: string
        hideDescription?: boolean
        hideTitle?: boolean
    }

    interface State {
        panelConfig:panelConfig
    }

    interface panelConfig{
        backgroundImageSrc?:string
        backgroundBlur?:number
        backgroundMaskNumber?:number
        logoText?:string
        clockShowSecond?:boolean
        searchBoxShow?:boolean
        marginTop?:number
        marginBottom?:number
        maxWidth?:number
        maxWidthUnit:string
        marginX?:number
        footerHtml?:string
        systemMonitorShow?:boolean
        systemMonitorShowTitle?:boolean
    }

    interface userConfig{
        panel:panelConfig
    }


}

