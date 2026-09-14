Event OnActivate(ObjectReference akActionRef)
	actor player = Game.GetPlayer()
	If akActionRef != player
		Return
	EndIf

	If BlockWhilePlayerIsInPowerArmor && player.IsInPowerArmor()
		If PowerArmorNoActivate
			PowerArmorNoActivate.Show()
		EndIf
		Return
	EndIf

	feralRef = GetReference().GetLinkedRef(FeralFurnitureLinkKeyword) as actor
	playerRef = player
	TryToSetStage()
EndEvent
