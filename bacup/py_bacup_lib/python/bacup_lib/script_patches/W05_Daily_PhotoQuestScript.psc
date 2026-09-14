Weapon Function PhotoCamera()
	Return Game.GetFormFromFile(0x0046F481, "SeventySix.esm") as Weapon
EndFunction

Function RegisterPhotoTarget(PhotoTargetsAndStages targetData)
	If targetData == None || targetData.TriggerAlias == None || targetData.StageToSet < 5
		Return
	EndIf
	If !IsStageDone(targetData.StageToSet - 5) || IsStageDone(targetData.StageToSet)
		Return
	EndIf
	ObjectReference targetRef = targetData.TriggerAlias.GetReference()
	Weapon camera = PhotoCamera()
	If targetRef != None && myPlayer != None && myPlayer.GetReference() != None && camera != None
		RegisterForHitEvent(targetRef, myPlayer, camera)
	EndIf
EndFunction

Function RefreshPhotoTargetRegistrations()
	UnregisterForAllHitEvents()
	If !IsRunning() || PhotoTargets == None
		Return
	EndIf
	Int targetIndex = 0
	While targetIndex < PhotoTargets.Length
		RegisterPhotoTarget(PhotoTargets[targetIndex])
		targetIndex += 1
	EndWhile
EndFunction

Event OnQuestInit()
	PlayerRef = myPlayer.GetActorReference()
	If PlayerRef != None
		RegisterForRemoteEvent(PlayerRef, "OnPlayerLoadGame")
	EndIf
	RefreshPhotoTargetRegistrations()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	RefreshPhotoTargetRegistrations()
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
	If akTarget != None && akAggressor == PlayerRef && akSource == PhotoCamera() && PhotoTargets != None
		Int targetIndex = 0
		While targetIndex < PhotoTargets.Length
			PhotoTargetsAndStages targetData = PhotoTargets[targetIndex]
			If targetData != None && targetData.TriggerAlias != None && targetData.TriggerAlias.GetReference() == akTarget && targetData.StageToSet >= 5 && IsStageDone(targetData.StageToSet - 5) && !IsStageDone(targetData.StageToSet)
				SetStage(targetData.StageToSet)
				RefreshPhotoTargetRegistrations()
				Return
			EndIf
			targetIndex += 1
		EndWhile
	EndIf
	RefreshPhotoTargetRegistrations()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == PlayerRef
		RefreshPhotoTargetRegistrations()
	EndIf
EndEvent

Event OnQuestShutdown()
	UnregisterForAllHitEvents()
	If PlayerRef != None
		UnregisterForRemoteEvent(PlayerRef, "OnPlayerLoadGame")
	EndIf
	PlayerRef = None
EndEvent
