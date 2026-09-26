Event OnRead()
    Actor player = Game.GetPlayer()
    If player != None && ValueToSet != None
        player.SetValue(ValueToSet, 1.0)
    EndIf

    EN05_PatriotismTrainingQuestScript course = Game.GetFormFromFile(0x0008C881, "SeventySix.esm") as EN05_PatriotismTrainingQuestScript
    If course != None && course.IsRunning()
        course.EN05PT_HandleDiaryRead(GetBaseObject() as Book)
    EndIf
EndEvent
