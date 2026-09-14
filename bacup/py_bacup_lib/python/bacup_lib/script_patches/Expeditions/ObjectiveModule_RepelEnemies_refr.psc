Function UpdateProgress(Float afCurrentProgressPercent)
	Int I = 0
	While I < ProgressTresholds.Length
		If afCurrentProgressPercent >= ProgressTresholds[I].ProgressThreshold_Percent
			CurrentAnim = ProgressTresholds[I].AnimationName
		EndIf
		I = I + 1
	EndWhile
	; Re-homed from OnSyncVariableNetworkChanged(CURRENTANIM_VARNAME): FO76 replicated
	; CurrentAnim to clients, which then played it. UpdateProgress is the only writer,
	; so the animation is driven directly from here in single-player.
	If CurrentAnim != "None" && Self.Is3DLoaded()
		Self.PlayAnimation(CurrentAnim)
	EndIf
EndFunction

Event OnLoad()
	If CurrentAnim == "None" && StartingAnim != "None"
		CurrentAnim = StartingAnim
	EndIf
	If CurrentAnim != "None"
		Self.PlayAnimation(CurrentAnim)
	EndIf
EndEvent

; @drop-member OnSyncVariableNetworkChanged
