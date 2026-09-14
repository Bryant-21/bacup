Event OnActivate(ObjectReference akActionRef)
	Actor playerRef = Game.GetPlayer()
	Quest owningQuest = GetOwningQuest()

	If akActionRef != playerRef || owningQuest == None
		Return
	EndIf
	If owningQuest.IsStageDone(StageToSet) || !owningQuest.IsStageDone(PrereqStage)
		Return
	EndIf
	If playerRef.GetEquippedWeapon() != Shovel
		If TWZ13_NoShovelMsg != None
			TWZ13_NoShovelMsg.Show()
		EndIf
		Return
	EndIf

	owningQuest.SetStage(StageToSet)
EndEvent
