Event OnLoad()
	If loopingSoundID == -1 && LoopingSoundToPlay != None && playSound && !Self.IsDisabled()
		loopingSoundID = LoopingSoundToPlay.Play(Self as ObjectReference)
	EndIf
EndEvent

; Re-homed from OnSyncVariableNetworkChanged("playSound"): FO76 replicated playSound
; and clients stopped the loop when it cleared. Nothing replicates in single-player,
; so the stop is exposed locally and driven from OnUnload.
Function StopLoopingSound()
	playSound = False
	If loopingSoundID != -1
		sound.StopInstance(loopingSoundID)
		loopingSoundID = -1
	EndIf
EndFunction

Event OnUnload()
	If loopingSoundID != -1
		sound.StopInstance(loopingSoundID)
	EndIf
	loopingSoundID = -1
EndEvent

; @drop-member OnSyncVariableNetworkChanged
