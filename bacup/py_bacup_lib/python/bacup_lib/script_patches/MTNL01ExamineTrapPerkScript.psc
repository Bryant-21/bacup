Event OnEntryRun(int auiEntryID, ObjectReference akTarget, Actor akOwner)
  if akTarget == None || akOwner == None
    return
  endif

  MTNL01QuestScript trapController = MTNL01_Raiders as MTNL01QuestScript
  if trapController == None
    return
  endif

  int disarmIndex = iMessageButtonDisarmIndex
  int takeIndex = iMessageButtonTakeIndex

  if disarmIndex == takeIndex
    disarmIndex = 0
    takeIndex = 1
  endif

  int choice = MTNL01_TrapWarningInt_Msg.Show()

  if choice == disarmIndex
    if akOwner.GetValue(Intelligence) < 5.0
      MTNL01_TrapWarningIntFail_Msg.Show()
      return
    endif

    trapController.DisarmTrap(akOwner)
  elseif choice == takeIndex
    trapController.TriggerTrap(akOwner)
    Utility.Wait(1.0)
    akTarget.Activate(akOwner, true)
  endif
EndEvent
