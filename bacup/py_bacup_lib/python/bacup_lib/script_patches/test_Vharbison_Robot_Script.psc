; OnPerkAdded is a FO76 actor event Fallout 4 never raises, and FO4 exposes no
; perk-added notification at all. BEHAVIOUR LOST: the fire effect shader no longer
; starts automatically when test_Vharbison_EventPerk is granted. The effect is
; exposed as a callable function so the granting quest can drive it. (Test script.)
; @drop-member OnPerkAdded

Function StartEventFireFX()
	utility.Wait(1.5)
	RobotFireFXS.Play(Self as objectreference, -1.0)
EndFunction
