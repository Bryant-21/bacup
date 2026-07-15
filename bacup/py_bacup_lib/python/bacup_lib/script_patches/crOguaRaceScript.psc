Function RegisterOguaShellEvents()
	If mySelf == None
		Return
	EndIf

	RegisterForAnimationEvent(mySelf, "TurnInvulnerable")
	RegisterForAnimationEvent(mySelf, "TurnVulnerable")
	If ShellEnterEvent != "" && ShellEnterEvent != "TurnInvulnerable"
		RegisterForAnimationEvent(mySelf, ShellEnterEvent)
	EndIf
EndFunction

Function UnregisterOguaShellEvents()
	If mySelf == None
		Return
	EndIf

	UnregisterForAnimationEvent(mySelf, "TurnInvulnerable")
	UnregisterForAnimationEvent(mySelf, "TurnVulnerable")
	If ShellEnterEvent != "" && ShellEnterEvent != "TurnInvulnerable"
		UnregisterForAnimationEvent(mySelf, ShellEnterEvent)
	EndIf
EndFunction

Function EnterShell()
	If mySelf == None || mySelf.IsDead() || ShellSpell == None || ShellLimit <= 0 || timesShelled >= ShellLimit
		Return
	EndIf
	If crOguaBlockShellsKeyword != None && mySelf.HasKeyword(crOguaBlockShellsKeyword)
		Return
	EndIf

	timesShelled += 1
	ShellSpell.Cast(mySelf, mySelf)
	If timesShelled >= ShellLimit && crOguaBlockShellsKeyword != None
		mySelf.AddKeyword(crOguaBlockShellsKeyword)
	EndIf
EndFunction

Function ExitShell()
	If mySelf != None && ShellSpell != None
		mySelf.DispelSpell(ShellSpell)
	EndIf
EndFunction

Event OnEffectStart(Actor akTarget, Actor akCaster)
	mySelf = akCaster
	If mySelf == None
		mySelf = akTarget
	EndIf
	timesShelled = 0
	If mySelf != None && !mySelf.IsDead()
		RegisterOguaShellEvents()
	EndIf
EndEvent

Event OnAnimationEvent(ObjectReference akSource, String asEventName)
	If mySelf == None || mySelf.IsDead() || akSource != mySelf
		Return
	EndIf

	If asEventName == "TurnInvulnerable" || (ShellEnterEvent != "" && asEventName == ShellEnterEvent)
		EnterShell()
	ElseIf asEventName == "TurnVulnerable"
		ExitShell()
	EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
	UnregisterOguaShellEvents()
	ExitShell()
	If mySelf != None && ShellLimit > 0 && timesShelled >= ShellLimit && crOguaBlockShellsKeyword != None
		mySelf.RemoveKeyword(crOguaBlockShellsKeyword)
	EndIf
	mySelf = None
EndEvent
