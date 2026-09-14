Bool Function IsDisguiseItem(Form item)
	Return item && DisguiseKeyword && item.HasKeyword(DisguiseKeyword)
EndFunction

Bool Function DisguiseIsEnabled(Quest owner)
	Return owner && owner.IsRunning() && (iDisguiseDisableStage == -1 || !owner.IsStageDone(iDisguiseDisableStage))
EndFunction

Event OnItemEquipped(Form akBaseObject, ObjectReference akReference)
	If !IsDisguiseItem(akBaseObject)
		Return
	EndIf

	Quest owner = GetOwningQuest()
	If !DisguiseIsEnabled(owner)
		Return
	EndIf

	Actor player = GetActorReference()
	If player && DisguiseFaction
		player.AddToFaction(DisguiseFaction)
	EndIf

	If (iOnEquip_PreReqStage == -1 || owner.IsStageDone(iOnEquip_PreReqStage)) && (iOnEquip_TurnOffStage == -1 || !owner.IsStageDone(iOnEquip_TurnOffStage))
		owner.SetStage(iOnEquip_SetStage)
	EndIf
EndEvent

Event OnItemUnequipped(Form akBaseObject, ObjectReference akReference)
	If IsDisguiseItem(akBaseObject)
		Actor player = GetActorReference()
		If player && DisguiseFaction
			player.RemoveFromFaction(DisguiseFaction)
		EndIf
	EndIf
EndEvent
