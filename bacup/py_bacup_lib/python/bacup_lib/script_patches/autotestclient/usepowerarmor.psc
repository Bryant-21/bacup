Function StartTest()
	utility.Wait(15.0)
	utility.Wait(5.0)
	form PowerArmorForm = game.GetForm(133022)
	If PowerArmorForm != None
		; FO76 Actor.FindClosestReferenceOfType(form, radius) -> FO4
		; Game.FindClosestReferenceOfTypeFromRef(form, center, radius).
		activePowerArmor = game.FindClosestReferenceOfTypeFromRef(PowerArmorForm, Self as objectreference, 500.0)
		If activePowerArmor != None
			Self.gotoState("waitForEnter")
			activePowerArmor.Activate(Self as objectreference, False)
			Self.StartTimer(EnterTimer, EnterTimerId)
		EndIf
	EndIf
EndFunction

Function TestEnteredArmor()
	utility.Wait(5.0)
	If Self.IsInPowerArmor()
		Self.gotoState("waitForExit")
		; FO76 Actor.ExitPowerArmor() has no FO4 equivalent; re-activating the frame
		; is the vanilla way to dismount.
		If activePowerArmor != None
			activePowerArmor.Activate(Self as objectreference, False)
		EndIf
		Self.StartTimer(ExitTimer, ExitTimerId)
	Else
		Self.gotoState("done")
	EndIf
EndFunction
