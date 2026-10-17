Scriptname B21:RandomEncounterStartup Extends Quest

; FO76's random encounters bind Fallout 4's own REScript, but the stage
; fragment that calls Startup() was stripped server-side, so nothing ever
; runs it. Startup() is what registers the quest for REParent's
; RECheckForCleanup event, so without it a converted encounter never stops:
; its actors persist, REParent's running-encounter budget fills permanently,
; and SendStoryEventAndWait then fails for every later encounter.
;
; FO76 also creates its encounter actors initially disabled and its own
; RE alias scripts enabled them; the stock FO4 RECollectionAliasScript and
; REAliasScript those aliases now bind never do, so the actors stayed hidden.
; Actors in RE-script aliases are enabled when the encounter begins: at
; REScript's RangeCheckStage when it has a range check, else at start.
; Aliases without an RE alias script keep their quest-staged reveal.
;
; FO76 assaults bind REAssaultQuestScript with array properties
; (Alias_Attackers/Alias_Defenders, AttackerLinks/DefenderLinks) that the
; stock FO4 script, which reads Attacker01Links..05, cannot bind, so its
; InitAssault/StartAssault find no actors. 85 of the 88 FO76 assaults put the
; attackers in collection alias 48 and the defenders in 49; the assault is run
; on those directly with the stock script's bound REAssaultRole and
; REAssaultFactionFriend: attackers stage (role 1), the range-check stage sends
; them in (role 2) and breaks the truce, stages 40/50 (defenders/attackers win)
; stand them down.

Event OnQuestInit()
	Quest OwningQuest = Self As Quest
	REScript Encounter = OwningQuest As REScript
	If Encounter
		Encounter.Startup()
		If WaitsForRangeCheck(Encounter) && !IsStageDone(Encounter.RangeCheckStage)
			SetAssaultRoles(1, False, False)
			Return
		EndIf
	EndIf
	EnableEncounterActors()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	REScript Encounter = (Self As Quest) As REScript
	If Encounter && WaitsForRangeCheck(Encounter) && auiStageID == Encounter.RangeCheckStage
		EnableEncounterActors()
		SetAssaultRoles(2, True, True)
	ElseIf auiStageID == 40 || auiStageID == 50
		SetAssaultRoles(0, False, False)
	EndIf
EndEvent

Function SetAssaultRoles(Int aiAttackerRole, Bool abBreakTruce, Bool abIncludeDefenders)
	REAssaultQuestScript Assault = (Self As Quest) As REAssaultQuestScript
	If !Assault || !Assault.REAssaultRole
		Return
	EndIf
	SetCollectionRole(GetAlias(48) As RECollectionAliasScript, aiAttackerRole, abBreakTruce, Assault)
	If abIncludeDefenders
		SetCollectionRole(GetAlias(49) As RECollectionAliasScript, 0, abBreakTruce, Assault)
	EndIf
EndFunction

Function SetCollectionRole(RefCollectionAlias akGroup, Int aiRole, Bool abBreakTruce, REAssaultQuestScript akAssault)
	If !akGroup
		Return
	EndIf
	Int index = akGroup.GetCount() - 1
	While index >= 0
		Actor member = akGroup.GetAt(index) As Actor
		If member
			member.SetValue(akAssault.REAssaultRole, aiRole)
			If abBreakTruce && akAssault.REAssaultFactionFriend
				member.RemoveFromFaction(akAssault.REAssaultFactionFriend)
			EndIf
			member.EvaluatePackage()
		EndIf
		index -= 1
	EndWhile
EndFunction

Bool Function WaitsForRangeCheck(REScript akEncounter)
	Return akEncounter.RangeCheckDistance > 0 && akEncounter.RangeCheckStage > 0
EndFunction

Function EnableEncounterActors()
	Int aliasID = 0
	While aliasID < 128
		Alias encounterAlias = GetAlias(aliasID)
		If encounterAlias
			RECollectionAliasScript collection = encounterAlias As RECollectionAliasScript
			If collection
				Int index = collection.GetCount() - 1
				While index >= 0
					EnableIfDisabled(collection.GetAt(index))
					index -= 1
				EndWhile
			Else
				REAliasScript single = encounterAlias As REAliasScript
				If single
					EnableIfDisabled(single.GetReference())
				EndIf
			EndIf
		EndIf
		aliasID += 1
	EndWhile
EndFunction

Function EnableIfDisabled(ObjectReference akRef)
	If akRef && akRef.IsDisabled() && !akRef.IsDeleted()
		akRef.Enable()
	EndIf
EndFunction
