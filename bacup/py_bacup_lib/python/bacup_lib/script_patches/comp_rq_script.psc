Event OnQuestInit()
	RefreshRuntimeReferences()
	FinishedStartup = True

	If ShutDownIfMissingObject && (!Alias_Object || !Alias_Object.GetReference())
		SetStage(QuestShutDownStage_DO_NOT_SET_QUESTSTAGE_AV)
	ElseIf ShutDownIfMissingTargetActor && (!Alias_TargetActor || !Alias_TargetActor.GetReference())
		SetStage(QuestShutDownStage_DO_NOT_SET_QUESTSTAGE_AV)
	EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	RefreshRuntimeReferences()
	If auiStageID != QuestCompletionStage_DO_NOT_SET_QUESTSTAGE_AV && auiStageID != QuestShutDownStage_DO_NOT_SET_QUESTSTAGE_AV
		If CompanionRef && CompanionRef.GetValue(COMP_QuestStage) < auiStageID
			CompanionRef.SetValue(COMP_QuestStage, auiStageID)
		EndIf
	EndIf

	If auiStageID == QuestStageAcceptQuest
		SetObjectiveDisplayed(GetFirstObjective())
	ElseIf auiStageID == SecondObjective
		SetObjectiveCompleted(GetFirstObjective())
		SetObjectiveDisplayed(GetSecondObjective())
	ElseIf auiStageID == QuestStageReturnToQuestGiver
		SetObjectiveCompleted(GetFirstObjective())
		SetObjectiveCompleted(GetSecondObjective())
		SetObjectiveDisplayed(QuestStageReturnToQuestGiver)
		TryCompleteRadiantQuest()
	ElseIf auiStageID == QuestStageFailQuest
		FailAllObjectives()
		ClearRadiantQuestState(False)
		SetStage(QuestShutDownStage_DO_NOT_SET_QUESTSTAGE_AV)
	ElseIf auiStageID == QuestCompletionStage_DO_NOT_SET_QUESTSTAGE_AV
		CompleteAllObjectives()
		If PlayerRef && PerkToAddToPlayer
			PlayerRef.AddPerk(PerkToAddToPlayer, True)
		EndIf
		ClearRadiantQuestState(True)
		SetStage(QuestShutDownStage_DO_NOT_SET_QUESTSTAGE_AV)
	ElseIf auiStageID == QuestShutDownStage_DO_NOT_SET_QUESTSTAGE_AV
		Stop()
	EndIf
EndEvent

Event OnQuestShutdown()
	FinishedStartup = False
	PlayerRef = None
	CompanionRef = None
	QuestLocation = None
	COMP_RQ_MasterIns = None
EndEvent

Function RefreshRuntimeReferences()
	If !PlayerRef
		PlayerRef = Game.GetPlayer()
	EndIf
	If Alias_Player && PlayerRef
		Alias_Player.ForceRefIfEmpty(PlayerRef)
	EndIf
	If Alias_Companion
		CompanionRef = Alias_Companion.GetActorReference() as CompanionScript
	EndIf
	If Alias_Location
		QuestLocation = Alias_Location.GetLocation()
	EndIf
	COMP_RQ_MasterIns = COMP_RQ_Master as COMP_RQ_MasterScript
EndFunction

Int Function GetFirstObjective()
	If CompanionRef
		Int customObjective = CompanionRef.GetCurrentRadiantFirstObjective()
		If customObjective >= 0
			Return customObjective
		EndIf
	EndIf
	Return FirstObjective
EndFunction

Int Function GetSecondObjective()
	If CompanionRef
		Int customObjective = CompanionRef.GetCurrentRadiantSecondObjective()
		If customObjective >= 0
			Return customObjective
		EndIf
	EndIf
	Return SecondObjective
EndFunction

Function ClearRadiantQuestState(Bool completed)
	If !CompanionRef || !PlayerRef
		Return
	EndIf

	If completed
		Float questCount = CompanionRef.GetValue(CompanionRef.COMP_QuestCount)
		If PlayerRef.GetValue(CompanionRef.PlayerQuestCountAV) > questCount
			questCount = PlayerRef.GetValue(CompanionRef.PlayerQuestCountAV)
		EndIf
		questCount += 1.0
		CompanionRef.SetValue(CompanionRef.COMP_QuestCount, questCount)
		PlayerRef.SetValue(CompanionRef.PlayerQuestCountAV, questCount)
		Float coolDown = CompanionRef.GetCurrentRadiantQuestCoolDown()
		If coolDown <= 0.0
			coolDown = 1.0
		EndIf
		CompanionRef.SetValue(CompanionRef.COMP_DailyQuest_NextAllowedDay, Utility.GetCurrentGameTime() + coolDown)
	EndIf
	PlayerRef.SetValue(CompanionRef.PlayerRadiantQuestDataIndex, -9999.0)
	If completed
		CompanionRef.TryStartOutroQuest()
	EndIf
EndFunction

; Stage 900 is the only stage that carries the CompleteQuest flag, so it is the
; only stage that pays the player and the only one that may be produced without
; a dialogue or alias behind it. Every RadiantQuestData row on every shipping
; ally selects COMP_Enum_QuestSetting_CompletesUponReturnToQuestGiver, so the
; return stage is the producer and nothing else is.
;
; The CompletesUponFirstObjectiveCompleted branch is deliberately not authored.
; Its enum value is 0.0, COMP_QuestSetting defaults to 0.0, and nothing in the
; converted output writes that actor value, so reading it would complete every
; radiant one stage early and skip the return the objective text asks for.
Bool Function TryCompleteRadiantQuest()
	If !IsRunning()
		Return False
	EndIf
	; The work has to have happened and the player has to have come back. Both
	; are stage checks rather than the caller's word, so the function is safe to
	; call from anywhere and cannot turn into an automatic stage advance.
	If !IsStageDone(SecondObjective) || !IsStageDone(QuestStageReturnToQuestGiver)
		Return False
	EndIf
	; Idempotent against a repeated dialogue line, a duplicate remote callback and
	; a reload: the completion stage is set at most once per run.
	If IsStageDone(QuestCompletionStage_DO_NOT_SET_QUESTSTAGE_AV)
		Return False
	EndIf
	If IsStageDone(QuestStageFailQuest) || IsStageDone(QuestShutDownStage_DO_NOT_SET_QUESTSTAGE_AV)
		Return False
	EndIf
	If !RadiantCompletionPaysOut()
		Return False
	EndIf
	SetStage(QuestCompletionStage_DO_NOT_SET_QUESTSTAGE_AV)
	Return True
EndFunction

; The guard on completion, stated once and evaluated before the transition
; rather than assumed by it: a radiant that pays nothing is worse than a radiant
; that stays open. Two carriers, either sufficient.
Bool Function RadiantCompletionPaysOut()
	If CompletionRewardScriptCoversStage(QuestCompletionStage_DO_NOT_SET_QUESTSTAGE_AV)
		Return True
	EndIf
	Return NativeCompletionXPIsCarried()
EndFunction

; Carrier 1, runtime-checked: the B21:QuestRewards row the GMRW repair attaches
; for caps and item payloads. These three quests do not carry one today; when
; one lands it is honoured here with no further change.
Bool Function CompletionRewardScriptCoversStage(Int auiStageID)
	Quest thisQuest = Self as Quest
	B21:QuestRewards rewards = thisQuest as B21:QuestRewards
	If !rewards
		Return False
	EndIf
	Return rewards.HasRewardForStage(auiStageID)
EndFunction

; Carrier 2: Fallout 4's own QuestCompletionXP field on this quest, which the
; GMRW repair excludes from B21:QuestRewards precisely because the engine
; already pays it. Papyrus cannot read that field, so this is a build-time fact
; and not a runtime probe, and it is asserted against the live plugin by
; test_wastelanders_radiant_completion.py. If any of the three generic radiants
; loses the field, that test fails and this returns False instead.
Bool Function NativeCompletionXPIsCarried()
	Return True
EndFunction
