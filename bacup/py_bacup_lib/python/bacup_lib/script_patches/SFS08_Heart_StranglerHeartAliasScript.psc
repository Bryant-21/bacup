Event OnAliasInit()
	ResolveHeart()
	LastDestStageSeen = 0
	DestructionLock = False
	CancelTimer(GasTimerID)
	ClearGas()
	If MyQuest != None
		UnregisterForRemoteEvent(MyQuest, "OnStageSet")
		RegisterForRemoteEvent(MyQuest, "OnStageSet")
	EndIf
	If HeartScript != None
		HeartScript.HeartState = 0
	EndIf
EndEvent

Event OnAliasShutdown()
	CancelTimer(GasTimerID)
	ClearGas()
	If MyQuest != None
		UnregisterForRemoteEvent(MyQuest, "OnStageSet")
	EndIf
EndEvent

Function ResolveHeart()
	If MyQuest == None
		MyQuest = GetOwningQuest()
	EndIf
	ObjectReference aliasRef = GetReference()
	If aliasRef != None && aliasRef != HeartRef
		HeartRef = aliasRef
		HeartScript = aliasRef as SFS08_Heart_StranglerHeartScript
	EndIf
EndFunction

Bool Function PlayerSeesHeartEvent()
	DefaultEventQuest eventQuest = MyQuest as DefaultEventQuest
	Return eventQuest == None || eventQuest.IsPlayerParticipating()
EndFunction

Function ShowHeartMessage(Message akMessage)
	If akMessage != None && PlayerSeesHeartEvent()
		akMessage.Show()
	EndIf
EndFunction

ObjectReference Function AliasReference(ReferenceAlias akAlias)
	If akAlias == None
		Return None
	EndIf
	Return akAlias.GetReference()
EndFunction

Int Function ReopenStageForClosing(Int aiClosingIndex)
	If aiClosingIndex == 0
		Return DestroyHeartStage02
	ElseIf aiClosingIndex == 1
		Return DestroyHeartStage03
	ElseIf aiClosingIndex == 2
		Return DestroyHeartStage04
	EndIf
	Return -1
EndFunction

; HeartData rows before the final (destroyed) row that carry an animation state close the heart;
; the Nth closing row reopens on DestroyHeartStage0(N+2).
Bool Function IsHeartClosed()
	ResolveHeart()
	If MyQuest == None || HeartData == None
		Return False
	EndIf
	Int closingIndex = 0
	Int dataIndex = 0
	While dataIndex < HeartData.Length - 1
		HeartAliasStruct entry = HeartData[dataIndex]
		If entry != None && entry.HeartAnimStateToSet != 0
			Int reopenStage = ReopenStageForClosing(closingIndex)
			If MyQuest.IsStageDone(entry.QuestStageToSet) && reopenStage >= 0 && !MyQuest.IsStageDone(reopenStage)
				Return True
			EndIf
			closingIndex += 1
		EndIf
		dataIndex += 1
	EndWhile
	Return False
EndFunction

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
	ProcessDestructionStages(aiCurrentStage)
EndEvent

; FO4 has no destructible DPS limit, so one hit can cross several thresholds. Rows apply in order and
; stop while the heart is closed; OpenHeart re-reads the live stage so skipped phases still run.
Function ProcessDestructionStages(Int aiReportedStage)
	ResolveHeart()
	If DestructionLock || MyQuest == None || HeartRef == None || HeartData == None
		Return
	EndIf
	DestructionLock = True
	Int reachedStage = aiReportedStage
	Int liveStage = HeartRef.GetCurrentDestructionStage()
	If liveStage > reachedStage
		reachedStage = liveStage
	EndIf
	While LastDestStageSeen < reachedStage && MyQuest.IsRunning() && !IsHeartClosed()
		LastDestStageSeen += 1
		ApplyDestructionStage(LastDestStageSeen)
		liveStage = HeartRef.GetCurrentDestructionStage()
		If liveStage > reachedStage
			reachedStage = liveStage
		EndIf
	EndWhile
	DestructionLock = False
EndFunction

Function ApplyDestructionStage(Int aiDestructionStage)
	Int dataIndex = HeartData.FindStruct("DestructionStage", aiDestructionStage)
	If dataIndex < 0
		Return
	EndIf
	HeartAliasStruct entry = HeartData[dataIndex]
	Bool heartKilled = dataIndex == HeartData.Length - 1

	If heartKilled && !MyQuest.IsStageDone(HeartAt25Stage) && !MyQuest.IsStageDone(NoBossStage)
		MyQuest.SetStage(NoBossStage)
	EndIf
	If entry.QuestStageToSet >= 0 && !MyQuest.IsStageDone(entry.QuestStageToSet)
		MyQuest.SetStage(entry.QuestStageToSet)
	EndIf

	If heartKilled
		KillHeart(entry.HeartAnimStateToSet)
	ElseIf entry.HeartAnimStateToSet != 0
		CloseHeart(entry.HeartAnimStateToSet)
	Else
		ShowHeartMessage(SFS08_Heart_ReinforcementsMessage)
		HeartRef.PlayAnimation("Attack")
		ReleaseGas()
	EndIf
EndFunction

Function CloseHeart(Int aiHeartAnimState)
	If HeartScript != None
		HeartScript.HeartState = aiHeartAnimState
	EndIf
	ObjectReference immuneRef = AliasReference(Alias_StranglerHeartImmune)
	If immuneRef != None
		immuneRef.Enable(False)
	EndIf
	HeartRef.Disable(False)
	ObjectReference holdingMarker = AliasReference(Alias_HoldingCellMarker)
	If holdingMarker != None
		HeartRef.MoveTo(holdingMarker)
	EndIf
	ShowHeartMessage(SFS08_Heart_ClosingMessage)
EndFunction

Function OpenHeart()
	ResolveHeart()
	If HeartRef == None
		Return
	EndIf
	ObjectReference immuneRef = AliasReference(Alias_StranglerHeartImmune)
	If !HeartRef.IsDisabled() && (immuneRef == None || immuneRef.IsDisabled())
		Return
	EndIf
	If HeartScript != None
		; State 0 resets the played flags, so the heart's own OnLoad replays its opening animation.
		HeartScript.HeartState = 0
	EndIf
	ObjectReference heartMarker = AliasReference(Alias_StranglerHeartMarker)
	If heartMarker != None
		HeartRef.MoveTo(heartMarker)
	EndIf
	HeartRef.Enable(False)
	If immuneRef != None
		immuneRef.Disable(False)
	EndIf
	ShowHeartMessage(SFS08_Heart_ReopeningMessage)
	ShowHeartMessage(SFS08_Heart_GasMessage)
	ReleaseGas()
	ProcessDestructionStages(0)
EndFunction

Function KillHeart(Int aiHeartAnimState)
	If HeartScript != None && aiHeartAnimState != 0
		HeartScript.HeartState = aiHeartAnimState
		HeartScript.CallFunctionNoWait("SetHeartStateClient", New Var[0])
	EndIf
EndFunction

Function ReleaseGas()
	CancelTimer(GasTimerID)
	ClearGas()
	ObjectReference gasFX = AliasReference(Alias_StranglerHeartGasFX)
	If gasFX != None
		gasFX.Enable(False)
	EndIf
	ObjectReference gasOrigin = AliasReference(Alias_CenterMarker)
	If gasOrigin == None
		gasOrigin = HeartRef
	EndIf
	If gasOrigin != None && SFS08_Heart_StranglerPoison != None
		ObjectReference poisonCloud = gasOrigin.PlaceAtMe(SFS08_Heart_StranglerPoison, 1, False, False, True)
		If poisonCloud != None && Alias_StranglerHeartGasDamage != None
			Alias_StranglerHeartGasDamage.ForceRefTo(poisonCloud)
		EndIf
	EndIf
	Actor playerRef = Game.GetPlayer()
	If SFS08_CameraShakeSpell != None && gasOrigin != None && playerRef != None && PlayerSeesHeartEvent()
		SFS08_CameraShakeSpell.Cast(gasOrigin, playerRef)
	EndIf
	StartTimer(GasTimerDuration, GasTimerID)
EndFunction

Function ClearGas()
	ObjectReference gasFX = AliasReference(Alias_StranglerHeartGasFX)
	If gasFX != None
		gasFX.Disable(False)
	EndIf
	If Alias_StranglerHeartGasDamage != None
		ObjectReference poisonCloud = Alias_StranglerHeartGasDamage.GetReference()
		Alias_StranglerHeartGasDamage.Clear()
		If poisonCloud != None
			poisonCloud.Disable(False)
			poisonCloud.Delete()
		EndIf
	EndIf
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID == GasTimerID
		ClearGas()
	EndIf
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
	If akSender != MyQuest
		Return
	EndIf
	If auiStageID == DestroyHeartStage02 || auiStageID == DestroyHeartStage03 || auiStageID == DestroyHeartStage04
		OpenHeart()
	EndIf
EndEvent
