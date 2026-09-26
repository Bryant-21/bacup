; FO76's SPECIAL loadout builds menu has no Fallout 4 equivalent; the one-time FO4 SPECIAL
; menu is its counterpart for a new character leaving Vault 76.
Event OnTriggerEnter(ObjectReference akActionRef)
	Actor playerRef = akActionRef as Actor
	If playerRef == Game.GetPlayer() && NPE_LoadoutsEnabled != None && NPE_LoadoutsEnabled.GetValue() > 0.0
		Quest owningQuest = GetOwningQuest()
		If owningQuest != None && SelectedLoadoutStage >= 0 && !owningQuest.GetStageDone(SelectedLoadoutStage)
			Game.ShowSPECIALMenu()
			owningQuest.SetStage(SelectedLoadoutStage)
		EndIf
	EndIf
EndEvent
