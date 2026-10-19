declare namespace File {

    interface Reference {
        type: 'wallpaper' | 'navigation' | 'group' | 'footer'
        recordId?: number
        title?: string
        field: string
        inTrash?: boolean
    }

	interface Info extends Common.InfoBase {
		src: string
		fileName: string
		method: number
		ext: string
        referenceCount?: number
        references?: Reference[]
	}


}
