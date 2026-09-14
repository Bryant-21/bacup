Event OnQuestInit()
	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef != None
		RegisterForRemoteEvent(playerRef, "OnKill")
	EndIf

	ReferenceAlias huntmasterAlias = GetAlias(2) as ReferenceAlias
	If huntmasterAlias != None && huntmasterAlias.GetReference() != None
		RegisterForRemoteEvent(huntmasterAlias.GetReference(), "OnActivate")
	EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActivator)
	ReferenceAlias huntmasterAlias = GetAlias(2) as ReferenceAlias
	If huntmasterAlias == None || akSender != huntmasterAlias.GetReference()
		Return
	EndIf
	If akActivator != Game.GetPlayer() || !IsStageDone(100) || IsStageDone(HuntStartStage)
		Return
	EndIf

	StartHunt()
EndEvent

Event Actor.OnKill(Actor akSender, Actor akVictim)
	If akSender != Game.GetPlayer() || akVictim == None
		Return
	EndIf
	If !IsStageDone(HuntStartStage) || IsStageDone(StageToSetAllBagged)
		Return
	EndIf

	Int huntIndex = 0
	While huntIndex < HuntSelected.Length
		If !HuntSelected[huntIndex].Bagged \
				&& HuntSelected[huntIndex].raceKeyword != None \
				&& akVictim.HasKeyword(HuntSelected[huntIndex].raceKeyword)
			SetStage(HuntSelected[huntIndex].StageToSet)
			Return
		EndIf
		huntIndex += 1
	EndWhile
EndEvent

Function StartHunt()
	If HuntSelected == None || HuntSelected.Length < 3
		Return
	EndIf
	If HuntOptions0 == None || HuntOptions0.Length == 0 \
			|| HuntOptions1 == None || HuntOptions1.Length == 0 \
			|| HuntOptions2 == None || HuntOptions2.Length == 0
		Return
	EndIf

	SelectHunt(0, HuntOptions0)
	SelectHunt(1, HuntOptions1)
	SelectHunt(2, HuntOptions2)
	SetStage(HuntStartStage)
EndFunction

Function SelectHunt(Int aiHuntIndex, HuntTypeOptions[] akOptions)
	Int selectedIndex = Utility.RandomInt(0, akOptions.Length - 1)
	HuntTypeOptions selectedHunt = akOptions[selectedIndex]

	HuntSelected[aiHuntIndex].Pick = selectedIndex
	HuntSelected[aiHuntIndex].Bagged = False
	HuntSelected[aiHuntIndex].raceKeyword = selectedHunt.raceKeyword
	HuntSelected[aiHuntIndex].RewardSpell = selectedHunt.RewardSpell

	LocationAlias creatureName = HuntSelected[aiHuntIndex].CreatureName
	If creatureName != None && selectedHunt.LocObjectiveString != None
		creatureName.ForceLocationTo(selectedHunt.LocObjectiveString)
	EndIf
EndFunction

Function CompleteHuntTarget(Int aiHuntIndex)
	If HuntSelected == None || aiHuntIndex < 0 || aiHuntIndex >= HuntSelected.Length
		Return
	EndIf
	If HuntSelected[aiHuntIndex].Bagged
		Return
	EndIf

	HuntSelected[aiHuntIndex].Bagged = True
	SetObjectiveCompleted(HuntSelected[aiHuntIndex].StageToSet)

	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef != None && HuntSelected[aiHuntIndex].RewardSpell != None
		HuntSelected[aiHuntIndex].RewardSpell.Cast(playerRef, playerRef)
	EndIf

	If AllTargetsBagged() && !IsStageDone(StageToSetAllBagged)
		SetStage(StageToSetAllBagged)
	EndIf
EndFunction

Bool Function AllTargetsBagged()
	If HuntSelected == None || HuntSelected.Length < 3
		Return False
	EndIf

	Int huntIndex = 0
	While huntIndex < HuntSelected.Length
		If !HuntSelected[huntIndex].Bagged
			Return False
		EndIf
		huntIndex += 1
	EndWhile
	Return True
EndFunction
