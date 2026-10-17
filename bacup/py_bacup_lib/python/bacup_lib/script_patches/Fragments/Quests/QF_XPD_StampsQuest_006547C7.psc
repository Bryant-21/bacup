; No dialogue line sets stage 200 ("Speak to Giuseppe") after conversion, so the quest
; listens for the player activating its Actor_Giuseppe alias (ID 1) (Tales routes his talk/trade).
Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(10, True)
	ReferenceAlias giuseppe = GetAlias(1) as ReferenceAlias
	If giuseppe != None && giuseppe.GetReference() != None
		RegisterForRemoteEvent(giuseppe.GetReference(), "OnActivate")
	EndIf
EndFunction

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer() && IsStageDone(100) && !IsStageDone(200)
		UnregisterForRemoteEvent(akSender, "OnActivate")
		SetStage(200)
	EndIf
EndEvent

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(10, True)
	SetStage(9000)
EndFunction

Function Fragment_Stage_9000_Item_00()
	CompleteAllObjectives()
	Stop()
EndFunction
