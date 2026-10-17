Event OnAliasInit()
	RegisterPlayerDistance()
EndEvent

Event OnLoad()
	RegisterPlayerDistance()
EndEvent

Function RegisterPlayerDistance()
	ObjectReference visitor = Self.GetReference()
	ObjectReference playerRef = None
	If Alias_Player
		playerRef = Alias_Player.GetReference()
	EndIf
	If !playerRef
		playerRef = Game.GetPlayer() as ObjectReference
	EndIf
	Quest owner = Self.GetOwningQuest()
	If visitor && owner && !owner.IsStageDone(StageToSetWhenDistanceReached)
		Self.RegisterForDistanceLessThanEvent(visitor as ScriptObject, playerRef as ScriptObject, IfIsAPlayerDistanceToSetStage)
	EndIf
EndFunction

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
	Quest owner = Self.GetOwningQuest()
	If owner && owner.IsRunning() && !owner.IsStageDone(StageToSetWhenDistanceReached)
		owner.SetStage(StageToSetWhenDistanceReached)
	EndIf
EndEvent
