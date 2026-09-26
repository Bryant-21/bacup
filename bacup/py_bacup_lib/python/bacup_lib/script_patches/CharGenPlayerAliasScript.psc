; Local half of FO76's CharGenPlayerActorScript, which the FO4 player base cannot carry.
; Timer 1 is the skeleton-reset fallback after the looks menu closes.

Function WatchFaceGen()
	RegisterForMenuOpenCloseEvent("LooksMenu")
	RegisterForAnimationEvent(GetActorReference(), "CharGenSkeletonReset")
EndFunction

Event OnMenuOpenCloseEvent(String asMenuName, Bool abOpening)
	If asMenuName == "LooksMenu" && !abOpening
		UnregisterForMenuOpenCloseEvent("LooksMenu")
		StartTimer(3.0, 1)
	EndIf
EndEvent

; MQ101 waits for this event before leaving the chargen skeleton, avoiding a visible pop.
Event OnAnimationEvent(ObjectReference akSource, String asEventName)
	If asEventName == "CharGenSkeletonReset"
		LeaveFaceGen()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == 1
		LeaveFaceGen()
	EndIf
EndEvent

Function LeaveFaceGen()
	Quest owningQuest = GetOwningQuest()
	If owningQuest.GetStageDone(200) || !owningQuest.GetStageDone(150)
		Return
	EndIf
	CancelTimer(1)
	Actor player = GetActorReference()
	UnregisterForAnimationEvent(player, "CharGenSkeletonReset")
	player.SetHasCharGenSkeleton(False)
	owningQuest.SetStage(200)
EndFunction

Event OnGetUp(ObjectReference akFurniture)
	CharGenQuestScript charGen = GetOwningQuest() as CharGenQuestScript
	If akFurniture == charGen.Alias_76CharGenPickUpPipBoy.GetRef() && charGen.GetStageDone(220) && !charGen.GetStageDone(charGen.AcquiredPipBoyStage)
		charGen.SetStage(charGen.AcquiredPipBoyStage)
	EndIf
EndEvent

; FO76 cleared the chargen state when the player left the chargen location.
Event OnLocationChange(Location akOldLoc, Location akNewLoc)
	If akOldLoc == ChargenLocation.GetLocation() && GetOwningQuest().GetStageDone(250)
		Game.SetInCharGen(False, False, False)
	EndIf
EndEvent
