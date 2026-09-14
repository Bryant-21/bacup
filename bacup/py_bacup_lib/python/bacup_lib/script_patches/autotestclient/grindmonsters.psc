; FO76 Actor.SetAIDriven()/SetWantSprinting() were QA-bot autopilot controls with no
; Fallout 4 equivalent. BEHAVIOUR LOST: this harness no longer drives the player under
; AI control, so the automated navigation tests will not move the character.
; FO76 Actor.FindRandomCombatTarget(radius) had no FO4 equivalent either; the closest
; vanilla analogue that does not need a base form is a direct combat check, so the
; harness now simply idles instead of seeking targets. BEHAVIOUR LOST: target seeking.

Function StartTest()
	Int time = 0
	utility.Wait(3.0)
	ammoForm = game.GetFormFromFile(127606, "SeventySix.esm")
	Self.FillAmmo()
	autotestclient:common.DistributeClients()
	utility.Wait(10.0)
	Self.StartTimer(time as Float, 0)
	While !isTestComplete
		utility.Wait(1.0)
	EndWhile
EndFunction

Function FillAmmo()
	; FO4 GetItemCount takes only the form.
	Int ammoCount = game.GetPlayer().GetItemCount(ammoForm)
EndFunction

Function Fight(actor akTarget)
	Int I = 0
	While I < 100 && akTarget != None && !akTarget.IsDead()
		utility.Wait(0.01)
		I = I + 1
	EndWhile
	Self.FillAmmo()
EndFunction
