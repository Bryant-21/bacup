Event OnLoad()
	If IdleToPlay && !Self.IsDead()
		PlayClientAnim()
	EndIf
EndEvent
