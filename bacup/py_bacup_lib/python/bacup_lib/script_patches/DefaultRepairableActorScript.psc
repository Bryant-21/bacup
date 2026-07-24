Event OnInit()
	If bHasInitialized
		Return
	EndIf
	bHasInitialized = True
	If WorkshopParent != None
		WorkshopParentInst = WorkshopParent as workshopparentscript
	EndIf
EndEvent
