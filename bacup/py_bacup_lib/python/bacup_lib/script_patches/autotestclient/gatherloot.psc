; FO76 Actor.SetAIDriven()/SetWantSprinting() were QA-bot autopilot controls with no
; Fallout 4 equivalent. BEHAVIOUR LOST: this harness no longer drives the player under
; AI control, so the automated navigation tests will not move the character.

Function StartTest()
	Int time = 0
	autotestclient:common.DistributeClients()
	Self.StartTimer(time as Float, 0)
	While !isTestComplete
		If time == 0
			Self.CollectContainer()
		Else
			Self.CollectFlora()
		EndIf
	EndWhile
EndFunction
