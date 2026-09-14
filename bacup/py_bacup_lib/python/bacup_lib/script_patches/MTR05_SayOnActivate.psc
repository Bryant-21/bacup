Event OnAliasInit()
	GoToState("ready")
EndEvent

State ready
	Event OnActivate(ObjectReference akActionRef)
		Actor playerRef = akActionRef as Actor
		If playerRef != Game.GetPlayer() || bAudioCooldownActive || !MTR_Hornwright_Industrial_Master.IsRunning() || !MTR05_Mother.IsRunning()
			Return
		EndIf
		If !MTR05_Mother.IsStageDone(50) && MTR05_Mother.GetStage() >= 30 && !playerRef.HasKeyword(MTR05_Main_PlayerHasCredentialsKeyword) && !playerRef.HasKeyword(MTR05_Main_PlayerIsExecKeyword)
			MTR05_Mother.SetStage(50)
		EndIf
		Topic responseTopic = MTR05_IDPrinterInvalidUser
		If playerRef.HasKeyword(MTR05_Main_PlayerIsExecKeyword)
			responseTopic = MTR05_SeniorExecDetected
		ElseIf playerRef.HasKeyword(MTR05_Main_PlayerHasCredentialsKeyword)
			responseTopic = MTR05_CardIssued
		ElseIf playerRef.GetItemCount(MTR05_HRPassword) > 0
			responseTopic = MTR05_ExecCredentialsRequired
		EndIf
		ObjectReference voiceRef = HiringSystemVoice.GetReference()
		If voiceRef != None
			voiceRef.Say(responseTopic, None, False, playerRef)
		EndIf
		bAudioCooldownActive = True
		GoToState("busy")
		StartTimer(iAudioCooldownLength as Float, iAudioCooldownID)
	EndEvent
EndState

State busy
	Event OnActivate(ObjectReference akActionRef)
	EndEvent

	Event OnTimer(Int aiTimerID)
		If aiTimerID == iAudioCooldownID
			bAudioCooldownActive = False
			GoToState("ready")
		EndIf
	EndEvent
EndState
