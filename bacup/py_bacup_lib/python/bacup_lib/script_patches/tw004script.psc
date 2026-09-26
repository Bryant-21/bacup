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
	; Backs up the three kill managers: FindKillManager returns None for any instance the
	; base-type cast cannot reach, and the three hunt groups use disjoint race keywords, so
	; a kill still advances exactly one slot whichever path sees it first.
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

	DefaultQuestOnKillManager killManager = FindKillManager(HuntSelected[aiHuntIndex].StageToSet)
	If killManager != None && selectedHunt.raceKeyword != None
		killManager.SetVictimKeyword(selectedHunt.raceKeyword)
		killManager.SetVictimsRequired(1)
		killManager.StartTrackingKills()
	EndIf
EndFunction

DefaultQuestOnKillManager Function FindKillManager(Int aiStageToSet)
	; Three DefaultQuestOnKillManager instances share this quest, so a plain cast to the
	; base type can land on any of them. Match on the instance's own bound StageToSet.
	Quest owner = Self as Quest
	DefaultQuestOnKillManagerB managerB = owner as DefaultQuestOnKillManagerB
	If managerB != None && managerB.StageToSet == aiStageToSet
		Return managerB
	EndIf
	DefaultQuestOnKillManagerC managerC = owner as DefaultQuestOnKillManagerC
	If managerC != None && managerC.StageToSet == aiStageToSet
		Return managerC
	EndIf
	; The base-type cast can resolve to B or C, which would hide a matching base instance,
	; so reject the two already identified before trusting its StageToSet.
	DefaultQuestOnKillManager managerBase = owner as DefaultQuestOnKillManager
	If managerBase != None && managerBase != (managerB as DefaultQuestOnKillManager) \
			&& managerBase != (managerC as DefaultQuestOnKillManager) \
			&& managerBase.StageToSet == aiStageToSet
		Return managerBase
	EndIf
	Return None
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
