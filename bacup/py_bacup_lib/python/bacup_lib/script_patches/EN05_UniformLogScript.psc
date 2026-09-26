Event OnRead()
    Actor player = Game.GetPlayer()
    If player != None && ValueToSet != None
        player.SetValue(ValueToSet, 1.0)
    EndIf

    EN05_QuestScript basic = Game.GetFormFromFile(0x0008C87F, "SeventySix.esm") as EN05_QuestScript
    If basic != None && basic.IsRunning()
        basic.EN05Basic_ReconcileUniform()
    EndIf
EndEvent
