State Ready
	Event OnActivate(ObjectReference akActionRef)
		Self.GotoState("Busy")
		; FO76 IsLocalPlayer() -> single-player identity test, and
		; Message.ShowSingle(receiver, ...) -> FO4 Message.Show() (FO4 always
		; targets the single player, so the receiver argument is dropped).
		If akActionRef == Game.GetPlayer()
			If iSystemState == 0
				MTR03_SystemState_01_EliminateMessage.Show()
			ElseIf iSystemState == 1
				MTR03_SystemState_02_RepairMessage.Show()
			ElseIf iSystemState == 2
				MTR03_SystemState_03_GrantOwnershipMessage.Show()
			ElseIf iSystemState == 3
				MTR03_SystemState_04_OwnedMessage.Show()
			ElseIf iSystemState == 4
				MTR03_SystemState_05_OwnedMessage.Show()
			EndIf
		EndIf
		Self.GotoState("Ready")
	EndEvent
EndState
