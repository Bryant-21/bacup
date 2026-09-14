Function ClientPlayGeneratorStartSound()
	GeneratorStartSound.play(ResourceContainer)
	; FO76 routed shakes through the per-client Player script; FO4 has the globals.
	Game.ShakeCamera(ResourceContainer, 0.25, 0.0)
	Game.ShakeController(0.25, 0.25, 0.25)
EndFunction

Function ClientRestartGenerator()
	Self.PlayAnimation("Press")
	utility.Wait(0.5)
	GeneratorStartSound.play(ResourceContainer)
	Game.ShakeCamera(ResourceContainer, 0.25, 0.0)
	Game.ShakeController(0.25, 0.25, 0.25)
	utility.Wait(1.0)
	Self.PlayAnimation("TurnOn01")
EndFunction

; Re-homed from OnSyncVariableNetworkChanged("GeneratorState"): FO76 replicated
; GeneratorState from the server and clients ran the "needs restart" sputter loop.
; Single-player has no replication, so the setter is exposed locally.
Function SetGeneratorState(Int iNewState)
	If GeneratorState == iNewState
		Return
	EndIf
	GeneratorState = iNewState
	If GeneratorState == -1
		While GeneratorState == -1
			GeneratorStopSound.play(ResourceContainer)
			Self.PlayAnimation("TurnOff01")
			utility.Wait(0.5)
			Self.PlayAnimation("TurnOn01")
			utility.Wait(0.5)
		EndWhile
	EndIf
EndFunction

; @drop-member OnSyncVariableNetworkChanged
