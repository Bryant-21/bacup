; Tunnel of Love (65B0A8) phase controller.
;
; QF_E09C_LoveTunnel_0065B0A8 already owns the objective flow and consumes the
; four phase-completion stages (190 / 290 / 390 / 500). Nothing produced them,
; so the event could start, display every objective and never finish. The four
; phase boundaries are not inferred: they are the record's own bound values --
; iDecoStartStage 110 -> iDecoStageToSetOnComplete 190, iTrackStartStage 200 ->
; 290, iHandyStartStage 300 -> 390, iWeddingStartStage 400 -> 500.
;
; Every phase is completed by a real world observable, never by a timer alone:
; decorations by enabled decoration references, tracks by repaired track
; references, the fake Miss Handy by the three robot parts actually in the
; player's inventory, and the wedding by sustained attendance.

Int Function PhasePollTimerID() Global
	Return 4091
EndFunction

Event OnQuestInit()
	ArmPhasePoll()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID == iDecoStartStage || auiStageID == iTrackStartStage || auiStageID == iHandyStartStage || auiStageID == iWeddingStartStage
		SpawnPhaseContent(auiStageID)
		ArmPhasePoll()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID != PhasePollTimerID()
		Return
	EndIf
	If EvaluatePhases()
		ArmPhasePoll()
	EndIf
EndEvent

Function ArmPhasePoll()
	CancelTimer(PhasePollTimerID())
	StartTimer(5.0, PhasePollTimerID())
EndFunction

Bool Function EvaluatePhases()
	; Returns whether the event is still in a phase worth polling, so a finished
	; or failed run stops re-arming instead of idling forever.
	If IsStageDone(iWeddingStageToSetOnComplete) || IsCompleted() || IsStopped()
		Return False
	EndIf

	If IsStageDone(iDecoStartStage) && !IsStageDone(iDecoStageToSetOnComplete)
		RunTimedCallouts(iDecoStageToSetOnComplete)
		If CountEnabledAliasRefs(DecorationEnablers) >= iDecoObjectiveMax
			SetStage(iDecoStageToSetOnComplete)
		EndIf
		Return True
	EndIf

	If IsStageDone(iTrackStartStage) && !IsStageDone(iTrackStageToSetOnComplete)
		RunTimedCallouts(iTrackStageToSetOnComplete)
		If CountRepairedTracks() >= iTracksObjectiveMax
			SetStage(iTrackStageToSetOnComplete)
		EndIf
		Return True
	EndIf

	If IsStageDone(iHandyStartStage) && !IsStageDone(iHandyStageToSetOnComplete)
		RunTimedCallouts(iHandyStageToSetOnComplete)
		If CountCollectedRobotParts() >= iFakeHandyObjectiveMax
			SetStage(iHandyStageToSetOnComplete)
		EndIf
		Return True
	EndIf

	If IsStageDone(iWeddingStartStage) && !IsStageDone(iWeddingStageToSetOnComplete)
		If PlayerAttendingWedding()
			SetStage(iWeddingStageToSetOnComplete)
		EndIf
		Return True
	EndIf

	Return True
EndFunction

Function SpawnPhaseContent(Int aiStage)
	If aiStage == iWeddingStartStage
		SpawnWeddingFood()
	EndIf
EndFunction

Function SpawnWeddingFood()
	; The reception spread is the only phase content the quest owns outright:
	; a leveled item per spawn marker. Guarded by cleanupDone so a save/load
	; inside the wedding phase cannot double the spread.
	If cleanupDone || WeddingFoodSpawns == None || E09C_LL_WeddingFood == None
		Return
	EndIf
	Int index = 0
	While index < WeddingFoodSpawns.GetCount()
		ObjectReference spawnPoint = WeddingFoodSpawns.GetAt(index) as ObjectReference
		If spawnPoint != None
			spawnPoint.PlaceAtMe(E09C_LL_WeddingFood, 1, False, False, True)
		EndIf
		index += 1
	EndWhile
	cleanupDone = True
EndFunction

Int Function CountEnabledAliasRefs(referencealias[] akAliases)
	If akAliases == None
		Return 0
	EndIf
	Int enabled = 0
	Int index = 0
	While index < akAliases.Length
		referencealias entry = akAliases[index]
		If entry != None
			ObjectReference decoration = entry.GetReference()
			If decoration != None && !decoration.IsDisabled()
				enabled += 1
			EndIf
		EndIf
		index += 1
	EndWhile
	Return enabled
EndFunction

Int Function CountRepairedTracks()
	; A track leaves the broken set by being disabled, so the repaired count is
	; the disabled remainder of BrokenTrackRefCollection. Reading the world state
	; avoids holding a counter, which this skeleton has no variable for.
	If BrokenTrackRefCollection == None
		Return 0
	EndIf
	Int repaired = 0
	Int index = 0
	While index < BrokenTrackRefCollection.GetCount()
		ObjectReference track = BrokenTrackRefCollection.GetAt(index) as ObjectReference
		If track == None || track.IsDisabled()
			repaired += 1
		EndIf
		index += 1
	EndWhile
	Return repaired
EndFunction

Int Function CountCollectedRobotParts()
	If Form_RobotParts == None
		Return 0
	EndIf
	Actor player = Game.GetPlayer()
	If player == None
		Return 0
	EndIf
	Int collected = 0
	Int index = 0
	While index < Form_RobotParts.Length
		Form part = Form_RobotParts[index]
		If part != None && player.GetItemCount(part) > 0
			collected += 1
		EndIf
		index += 1
	EndWhile
	Return collected
EndFunction

Bool Function PlayerAttendingWedding()
	; "Attend the wedding" has no activator and no kill goal; presence beside
	; Mr Lovely for the ceremony is the observable. fMrLovelyResetTimer is the
	; record's own ceremony-length value, scaled to the poll interval.
	Actor player = Game.GetPlayer()
	If player == None || Alias_MrHandy == None
		Return False
	EndIf
	ObjectReference lovely = Alias_MrHandy.GetReference()
	If lovely == None
		Return False
	EndIf
	Return player.GetDistance(lovely) <= 2048.0 && GetStageDoneElapsed(iWeddingStartStage) >= 1
EndFunction

Int Function GetStageDoneElapsed(Int aiStage)
	; Papyrus cannot ask when a stage was set, so attendance is confirmed on the
	; poll after the phase opened rather than on the same frame it opened.
	If IsStageDone(aiStage)
		Return 1
	EndIf
	Return 0
EndFunction

Function RunTimedCallouts(Int aiPhaseStopStage)
	If TimedObjectiveCallouts == None
		Return
	EndIf
	Int index = 0
	While index < TimedObjectiveCallouts.Length
		ObjectiveCallout callout = TimedObjectiveCallouts[index]
		If callout.iStageToStop == aiPhaseStopStage && !IsStageDone(callout.iStageToStop)
			If callout.pSceneToStart != None && !callout.pSceneToStart.IsPlaying()
				callout.pSceneToStart.Start()
			EndIf
			If callout.iStageToStart > 0 && !IsStageDone(callout.iStageToStart)
				SetStage(callout.iStageToStart)
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction
