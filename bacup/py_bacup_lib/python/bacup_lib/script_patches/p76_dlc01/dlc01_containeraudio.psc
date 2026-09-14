Event OnLoad()
	; OnItemAdded/OnItemRemoved below never fire without a filter registration, and
	; the registration does not survive a cell unload -- re-arm it on every load.
	AddInventoryEventFilter(None)
	If loopingSoundID == -1 && LoopingSoundToPlay != None && playSound
		loopingSoundID = LoopingSoundToPlay.Play(Self as ObjectReference)
	EndIf
EndEvent

; Re-homed from OnSyncVariableNetworkChanged("playSound"): FO76 replicated playSound
; and clients stopped the loop when it cleared. The only writer implied by the
; surviving declarations is StopOnInventoryChange, so the stop is driven from the
; container's own FO4 inventory events instead.
Function StopLoopingSound()
	playSound = False
	If loopingSoundID != -1
		sound.StopInstance(loopingSoundID)
		loopingSoundID = -1
	EndIf
EndFunction

Event OnItemAdded(form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
	If StopOnInventoryChange
		Self.StopLoopingSound()
	EndIf
EndEvent

Event OnItemRemoved(form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
	If StopOnInventoryChange
		Self.StopLoopingSound()
	EndIf
EndEvent

Event OnUnload()
	If loopingSoundID != -1
		sound.StopInstance(loopingSoundID)
	EndIf
	loopingSoundID = -1
EndEvent

; @drop-member OnSyncVariableNetworkChanged
